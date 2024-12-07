ARCH_LIBDIR ?= /lib/$(shell $(CC) -dumpmachine)

SELF_EXE = target/release/s3n-avs-node

.PHONY: all
all: $(SELF_EXE) s3n-avs-node.manifest
ifeq ($(SGX),1)
all: s3n-avs-node.manifest.sgx s3n-avs-node.sig
endif

ifeq ($(DEBUG),1)
GRAMINE_LOG_LEVEL = debug
else
GRAMINE_LOG_LEVEL = error
endif

# Note that we're compiling in release mode regardless of the DEBUG setting passed
# to Make, as compiling in debug mode results in an order of magnitude's difference in
# performance that makes testing by running a benchmark with ab painful. The primary goal
# of the DEBUG setting is to control Gramine's loglevel.
-include $(SELF_EXE).d # See also: .cargo/config.toml
$(SELF_EXE): Cargo.toml 
	cargo build --release

RA_TYPE ?= epid
RA_CLIENT_SPID ?= 12345678901234567890123456789012
RA_CLIENT_LINKABLE ?= 0

GIT_HASH=$(shell git rev-parse HEAD)
ECDSA_KEY=$(HOME)/.eigenlayer/operator_keys/s3n.ecdsa.key.json
BLS_KEY=$(HOME)/.eigenlayer/operator_keys/s3n.bls.key.json

s3n-avs-node.manifest: s3n-avs-node.manifest.template
	gramine-manifest \
		-Dlog_level=$(GRAMINE_LOG_LEVEL) \
		-Darch_libdir=$(ARCH_LIBDIR) \
		-Dself_exe=$(SELF_EXE) \
		-Dra_type=$(RA_TYPE) \
		-Dra_client_spid=$(RA_CLIENT_SPID) \
		-Dra_client_linkable=$(RA_CLIENT_LINKABLE) \
		$< $@

# Make on Ubuntu <= 20.04 doesn't support "Rules with Grouped Targets" (`&:`),
# see the helloworld example for details on this workaround.
s3n-avs-node.manifest.sgx s3n-avs-node.sig: sgx_sign
	@:

.INTERMEDIATE: sgx_sign
sgx_sign: s3n-avs-node.manifest $(SELF_EXE)
	gramine-sgx-sign \
		--manifest $< \
		--output $<.sgx

ifeq ($(SGX),)
GRAMINE = gramine-direct
else
GRAMINE = gramine-sgx
endif

.PHONY: start-gramine-server
start-node: all
	$(GRAMINE) s3n-avs-node --config-file ./config/config.yaml

.PHONY: clean
clean:
	$(RM) -rf *.token *.sig *.manifest.sgx *.manifest result-* OUTPUT

.PHONY: distclean
distclean: clean
	$(RM) -rf target/ Cargo.lock

.PHONY: docker-build
docker-build:
	docker build . --tag s3nlabseulerlagrange/s3n-avs-node:$(GIT_HASH)
	docker push s3nlabseulerlagrange/s3n-avs-node:$(GIT_HASH)

