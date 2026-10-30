# Milestone gates (see plan-rust.md §6). Each exits nonzero on any failure or
# missing required data/tool. Local golden data lives in .golden/ (gitignored).
ORACLE  ?= .golden/reproinx.py
VALIDATOR ?= $(shell command -v bids-validator)
BIN := target/release/reproinx
WASM_ENV := CARGO_PROFILE_RELEASE_OPT_LEVEL=z CARGO_PROFILE_RELEASE_PANIC=abort CARGO_PROFILE_RELEASE_STRIP=false

.PHONY: build test lint wasm oracle synth golden golden-variants validate \
	check-m00 check-m01 check-m02 check-m03 check-m04 check-m05 check-m06 check-m07 \
	check-m08 check-m09 check-m10 check-m11 check-m12 check-m13 check-m14 check-m15

build:
	cargo build --release --locked -p reproinx

test:
	cargo test --locked --workspace

lint:
	cargo clippy --locked --workspace --all-targets -- -D warnings
	cargo fmt --all --check

wasm:
	$(WASM_ENV) wasm-pack build wasm --target web --release --out-dir ../js/pkg --no-pack

# Pinned Python oracle (dcm2niix tools/reproinx.py @ 428fabe9).
oracle: $(ORACLE)
$(ORACLE):
	mkdir -p .golden
	curl -fsSL https://raw.githubusercontent.com/rordenlab/dcm2niix/428fabe9da66aa73737a0abee8529764eee7edcb/tools/reproinx.py -o $@

synth: build oracle
	python3 -I tests/synth/run.py $(ORACLE) $(BIN)

golden: build
	tests/golden/run.sh

golden-variants: build
	for f in "" "--shift-dates" "--keep-derivatives" "-N 10" "--strict" "--shift-dates --keep-derivatives"; do \
	  FLAGS="$$f" tests/golden/run.sh || exit 1; done

validate: golden
	test -n "$(VALIDATOR)" || { echo "set VALIDATOR=bids-validator 2.x"; exit 1; }
	tests/golden/validate.sh "$(VALIDATOR)"

check-m00:
	tests/golden/check_m0.sh

check-m01: test lint wasm

check-m02: check-m01 synth
	python3 -I tests/golden/selftest.py

check-m03 check-m04 check-m05 check-m06 check-m07 check-m08 check-m09 check-m10: check-m02 golden

check-m11: check-m10 golden-variants validate

# Real browser: PLAYWRIGHT = playwright package dir. The converter is the
# bundled published @niivue/dcm2niix (js/dcm2niix, verified by SHA256SUMS).
XA60E ?= /Users/chris/src/bidsui/datasets/reproinXA60e_DICOM
BROWSER_DICOM ?= $(XA60E)/20260512115434_DR
check-m12: check-m11
	cd js && node --test
	test -d "$(PLAYWRIGHT)" || { echo "set PLAYWRIGHT"; exit 1; }
	node js/tools/browser.mjs $(BROWSER_DICOM) .golden/browser.zip $(PLAYWRIGHT)
	rm -rf .golden/browser .golden/browser-native && mkdir -p .golden/browser && cd .golden/browser && unzip -q ../browser.zip
	$(BIN) $(BROWSER_DICOM) .golden/browser-native > /dev/null 2>&1
	python3 -I tests/golden/browser_compare.py .golden/browser-native .golden/browser

# BIDSvue (worktree on branch reproin-rs until merged): full pre-push check,
# then bridge + TS apply vs every golden reference, with undo and rollback.
BIDSVUE ?= ../BIDSvue-reproin
check-m13: check-m12
	test -d $(BIDSVUE)/src-tauri || { echo "BIDSvue checkout missing: $(BIDSVUE)"; exit 1; }
	cd $(BIDSVUE) && bun run check
	cd $(BIDSVUE) && bun scripts/reproin-e2e.ts $(XA60E) $(CURDIR)/.golden/reproinXA60e/ref $(CURDIR)/tests/golden/compare.py
	for g in $(CURDIR)/.golden/*/; do test -d $$g/ref || continue; \
	  (cd $(BIDSVUE) && bun scripts/reproin-e2e.ts --raw=$$g/raw $$g/ref $(CURDIR)/tests/golden/compare.py) || exit 1; done

# webapps app (worktree on branch reproin-app until merged).
WEBAPPS ?= ../webapps-reproin
check-m14: check-m12
	cd $(WEBAPPS) && pnpm build && pnpm --filter reproin test
	cd $(WEBAPPS)/apps/reproin && REPROIN_E2E_DICOM_DIR=$(BROWSER_DICOM) npx playwright test
	cd $(WEBAPPS) && SMOKE_APPS=reproin pnpm audit:interfaces && SMOKE_APPS=reproin pnpm test:mobile && SMOKE_APPS=reproin pnpm test:interface-workflows

# Release readiness (publishing stays manual): versions agree, packages build,
# retirement patch applies to the dcm2niix checkout.
DCM2NIIX_REPO ?= ../dcm2niix
check-m15: check-m13 check-m14
	v=$$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2); grep -q "\"version\": \"$$v\"" js/package.json || { echo "js/package.json version != $$v"; exit 1; }
	cd js && npm pack --dry-run > /dev/null
	git -C $(DCM2NIIX_REPO) apply --check -p1 $(CURDIR)/release/dcm2niix-reproinx-deprecation.patch
