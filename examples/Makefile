# stave examples: synthetic Wiz fixtures + 3 asserts.
#
# The asserts simulate the v0.1 primitive flows in jq + shell, so they run
# with no binary, no network, and no tenant credentials. When a built
# binary is present under ../target they additionally cross-check the real
# primitive's output against the simulation on the same fixtures, which is
# the regression contract in both directions.
#
#   make assert        # jq simulation, plus a cross-check if a binary exists
#   make assert-built  # build the binary first, then cross-check for real

.PHONY: assert assert-01 assert-02 assert-03 assert-built

assert: assert-01 assert-02 assert-03
	@echo
	@echo "all asserts passed."

assert-01:
	@bash asserts/01-round-trip.sh

assert-02:
	@bash asserts/02-cross-kind-enrich.sh

assert-03:
	@bash asserts/03-rank-stability.sh

assert-built:
	@cd .. && cargo build -p stave-cli
	@$(MAKE) assert
