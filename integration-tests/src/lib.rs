//! integration tests for s3n

#[cfg(test)]
mod tests {

    use eigen_logging::{init_logger, log_level::LogLevel};
    use s3n_config::S3NConfig;
    use std::{
        process::Stdio,
        sync::Arc,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;
    use alloy::primitives::{FixedBytes, U256};
    use eigen_testing_utils::anvil_constants::{
        get_avs_directory_address, get_delegation_manager_address, get_strategy_manager_address,
    };
    use eigen_utils::get_provider;
    use s3n_aggregator::Aggregator;
    use s3n_bindings::S3NTaskManager;
    use s3n_challenger::Challenger;
    use s3n_operator::builder::OperatorBuilder;
    use s3n_avs::commands::avs::register_operator_with_el_and_avs;
    use s3n_testing_utils::{
        get_s3n_operator_state_retriever,
        get_s3n_registry_coordinator, get_s3n_strategy_address,
        get_s3n_task_manager,
    };
    use serial_test::serial;

    const S3N_CONFIG_FILE: &str = r#"
    [rpc_config]
    chain_id = 31337
    http_rpc_url = "http://localhost:8545"
    ws_rpc_url = "ws://localhost:8545"
    signer = "0x2a871d0798f97d79848a013d4936a73bf4cc922c825d33c1cf7073dff6d409c6"

    [ecdsa_config]
    keystore_path = "../crates/testing-utils/src/ecdsakeystore.json"
    keystore_password = "test"

    [bls_config]
    keystore_path = "../crates/testing-utils/src/blskeystore.json"
    keystore_password = "testpassword"

    [operator_config]
    operator_address = "0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266"
    operator_id = "0xb345f720903a3ecfd59f3de456dd9d266c2ce540b05e8c909106962684d9afa3"
    
    [aggregator_config]
    ip_address = "127.0.0.1:8080"

    [operator_registration_config]
    register_operator = true
    operator_pvt_key = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
    operator_to_avs_registration_sig_salt = "b345f720903a3ecfd59f3de456dd9d266c2ce540b05e8c909106962684d9afa3"
    socket = "s3n"
    quorum_number = "00"
    sig_expiry = "10"
    "#;

    async fn register_operator_with_el() {
        let mut s3n_config: S3NConfig =
            toml::from_str(S3N_CONFIG_FILE).unwrap();

        s3n_config.set_registry_coordinator_addr(
            get_s3n_registry_coordinator()
                .await
                .to_string(),
        );
        s3n_config.set_operator_state_retriever(
            get_s3n_operator_state_retriever()
                .await
                .to_string(),
        );
        let delegation_manager_address_anvil = get_delegation_manager_address().await;
        let avs_directory_address_anvil = get_avs_directory_address().await;

        let strategy_manager_address_anvil = get_strategy_manager_address().await;
        let erc20_mock_strategy_address_anvil = get_s3n_strategy_address().await;

        s3n_config.set_delegation_manager_addr(delegation_manager_address_anvil.to_string());
        s3n_config.set_avs_directory_address(avs_directory_address_anvil.to_string());
        s3n_config.set_strategy_manager_addr(strategy_manager_address_anvil.to_string());
        s3n_config
            .set_erc20_mock_strategy_address(erc20_mock_strategy_address_anvil.to_string());

        let now = SystemTime::now();
        let mut expiry: U256 = U256::from(0);
        if let Ok(duration_since_epoch) = now.duration_since(UNIX_EPOCH) {
            let seconds = duration_since_epoch.as_secs(); // Returns a u64

            // Signature expiry is at 10000 seconds
            expiry = U256::from(seconds) + U256::from(10000);
        } else {
            println!("System time seems to be before the UNIX epoch.");
        }

        s3n_config.set_sig_expiry(expiry.to_string());
        let _ = register_operator_with_el_and_avs(
            s3n_config.operator_pvt_key(),
            s3n_config.http_rpc_url(),
            s3n_config.ecdsa_keystore_path(),
            s3n_config.ecdsa_keystore_password(),
            s3n_config.registry_coordinator_addr().unwrap(),
            s3n_config.operator_state_retriever_addr().unwrap(),
            s3n_config.delegation_manager_addr().unwrap(),
            s3n_config.avs_directory_addr().unwrap(),
            s3n_config.strategy_manager_addr().unwrap(),
            s3n_config.erc20_mock_strategy_addr().unwrap(),
            &s3n_config.bls_keystore_path(),
            &s3n_config.bls_keystore_password(),
            s3n_config
                .operator_to_avs_registration_sig_salt()
                .unwrap(),
            s3n_config.sig_expiry().unwrap(),
            s3n_config.quorum_number().unwrap(),
            s3n_config.socket().to_string(),
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    #[serial]
    async fn test_s3n_without_challenger() {
        init_logger(LogLevel::Info);
        register_operator_with_el().await;
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        let mut s3n_config: S3NConfig =
            toml::from_str(S3N_CONFIG_FILE).unwrap();
        s3n_config.set_aggregator_ip_address("127.0.0.1:8081".to_string());
        s3n_config.set_registry_coordinator_addr(
            get_s3n_registry_coordinator()
                .await
                .to_string(),
        );
        s3n_config.set_operator_state_retriever(
            get_s3n_operator_state_retriever()
                .await
                .to_string(),
        );

        let mut operator_builder = OperatorBuilder::build(s3n_config.clone())
            .await
            .unwrap();

        let _ = tokio::spawn(async move {
            let _ = operator_builder.start_operator().await;
        });

        let aggregator = Aggregator::new(s3n_config.clone()).await.unwrap();

        let arc_agg = Arc::new(tokio::sync::Mutex::new(aggregator));
        let arc_agg_clone = Arc::clone(&arc_agg);

        // Run process_tasks in a separate thread
        let _ = std::thread::spawn(move || {
            tokio::runtime::Runtime::new().unwrap().block_on(async {
                if let Err(e) =
                    Aggregator::process_tasks("ws://localhost:8545".to_string(), arc_agg_clone)
                        .await
                {
                    eprintln!("Process tasks error: {:?}", e);
                }
            });
        });

        // Run the server in a separate thread
        let _ = std::thread::spawn(move || {
            tokio::runtime::Runtime::new().unwrap().block_on(async {
                if let Err(e) = Aggregator::start_server(Arc::clone(&arc_agg)).await {
                    eprintln!("Server error: {:?}", e);
                }
            });
        });
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

        let task_generator = s3n_task_generator::TaskManager::new(
            get_s3n_task_manager().await,
            "http://localhost:8545".to_string(),
            "0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d".to_string(),
        );
        let _ = task_generator.create_new_task("2".parse().unwrap()).await;
        tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;

        let task_manager_contract = S3NTaskManager::new(
            get_s3n_task_manager().await,
            get_provider("http://localhost:8545"),
        );
        let latest_task_num = task_manager_contract
            .latestTaskNum()
            .call()
            .await
            .unwrap()
            ._0;

        let task_hash = task_manager_contract
            .allTaskHashes(latest_task_num - 1)
            .call()
            .await
            .unwrap()
            ._0;
        assert_ne!(FixedBytes::<32>::default(), task_hash);

        tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;

        let response_hash = task_manager_contract
            .allTaskResponses(latest_task_num - 1)
            .call()
            .await
            .unwrap()
            ._0;
        assert_ne!(FixedBytes::<32>::default(), response_hash);

        let is_challenge_success = task_manager_contract
            .taskSuccesfullyChallenged(latest_task_num - 1)
            .call()
            .await
            .unwrap()
            ._0;

        assert_eq!(is_challenge_success, false);
    }

    #[tokio::test]
    #[serial]
    async fn test_s3n_with_challenger() {
        init_logger(LogLevel::Info);
        register_operator_with_el().await;
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

        let mut s3n_config: S3NConfig =
            toml::from_str(S3N_CONFIG_FILE).unwrap();
        s3n_config.set_aggregator_ip_address("127.0.0.1:8082".to_string());

        s3n_config.set_registry_coordinator_addr(
            get_s3n_registry_coordinator()
                .await
                .to_string(),
        );
        s3n_config.set_operator_state_retriever(
            get_s3n_operator_state_retriever()
                .await
                .to_string(),
        );

        let mut operator_builder = OperatorBuilder::build(s3n_config.clone())
            .await
            .unwrap();

        let _ = tokio::spawn(async move {
            let _ = operator_builder.start_operator().await;
        });

        let aggregator = Aggregator::new(s3n_config.clone()).await.unwrap();

        let arc_agg = Arc::new(tokio::sync::Mutex::new(aggregator));
        let arc_agg_clone = Arc::clone(&arc_agg);

        // Run process_tasks in a separate thread
        let _ = std::thread::spawn(move || {
            tokio::runtime::Runtime::new().unwrap().block_on(async {
                if let Err(e) =
                    Aggregator::process_tasks("ws://localhost:8545".to_string(), arc_agg_clone)
                        .await
                {
                    eprintln!("Process tasks error: {:?}", e);
                }
            });
        });

        // Run the server in a separate thread
        let _ = std::thread::spawn(move || {
            tokio::runtime::Runtime::new().unwrap().block_on(async {
                if let Err(e) = Aggregator::start_server(Arc::clone(&arc_agg)).await {
                    eprintln!("Server error: {:?}", e);
                }
            });
        });
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

        let mut challenger = Challenger::build(s3n_config).await.unwrap();
        let c_handle = tokio::spawn(async move {
            let _ = challenger.start_challenger().await;
        });

        let task_generator = s3n_task_generator::TaskManager::new(
            get_s3n_task_manager().await,
            "http://localhost:8545".to_string(),
            "0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d".to_string(),
        );
        let _ = task_generator.create_new_task("2".parse().unwrap()).await;
        tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;

        let task_manager_contract = S3NTaskManager::new(
            get_s3n_task_manager().await,
            get_provider("http://localhost:8545"),
        );
        let latest_task_num = task_manager_contract
            .latestTaskNum()
            .call()
            .await
            .unwrap()
            ._0;

        let task_hash = task_manager_contract
            .allTaskHashes(latest_task_num - 1)
            .call()
            .await
            .unwrap()
            ._0;
        assert_ne!(FixedBytes::<32>::default(), task_hash);

        tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;

        let response_hash = task_manager_contract
            .allTaskResponses(latest_task_num - 1)
            .call()
            .await
            .unwrap()
            ._0;
        assert_ne!(FixedBytes::<32>::default(), response_hash);

        let is_challenge_success = task_manager_contract
            .taskSuccesfullyChallenged(latest_task_num - 1)
            .call()
            .await
            .unwrap()
            ._0;

        assert_eq!(is_challenge_success, true);
    }
}
