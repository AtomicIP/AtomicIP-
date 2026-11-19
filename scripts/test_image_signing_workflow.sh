#!/usr/bin/env bash

################################################################################
# Tests for #1257: image signing, SBOM and provenance in gitops-promote.yml
#
# Checks that the build job signs the pushed digest, attaches an SBOM, attests
# provenance, and that the production promotion verifies the signature before
# opening the promotion pull request.
#
# Run with: ./scripts/test_image_signing_workflow.sh
################################################################################

set -u

WORKFLOW="$(cd "$(dirname "$0")/.." && pwd)/.github/workflows/gitops-promote.yml"
failures=0

check() {
    local name="$1"; shift
    if "$@"; then
        echo "PASS  $name"
    else
        echo "FAIL  $name"
        failures=$((failures + 1))
    fi
}

# Print the step list of one job as JSON.
job_steps() {
    python3 -c '
import json, sys, yaml
wf = yaml.safe_load(open(sys.argv[1]))
print(json.dumps(wf["jobs"][sys.argv[2]]["steps"]))
' "$WORKFLOW" "$1"
}

step_text() {
    job_steps "$1" | python3 -c '
import json, sys
steps = json.load(sys.stdin)
print("\n".join(json.dumps(s) for s in steps))
'
}

test_workflow_has_oidc_and_attestation_permissions() {
    python3 -c '
import sys, yaml
perms = yaml.safe_load(open(sys.argv[1]))["permissions"]
assert perms.get("id-token") == "write", perms
assert perms.get("attestations") == "write", perms
' "$WORKFLOW"
}

test_build_signs_pushed_digest() {
    step_text build-and-stage | grep -q 'cosign sign --yes' \
        && step_text build-and-stage | grep -q 'DIGEST_REF'
}

test_build_generates_sbom_attestation() {
    local steps; steps="$(step_text build-and-stage)"
    grep -q 'anchore/sbom-action' <<< "$steps" \
        && grep -q 'cosign attest --yes --type spdxjson' <<< "$steps"
}

test_build_attests_provenance() {
    step_text build-and-stage | grep -q 'actions/attest-build-provenance'
}

test_signing_uses_built_digest() {
    step_text build-and-stage | grep -q 'steps.build.outputs.digest'
}

test_promotion_verifies_signature_before_pr() {
    python3 -c '
import sys, yaml
steps = yaml.safe_load(open(sys.argv[1]))["jobs"]["promote-production"]["steps"]
names = [str(s.get("uses", "")) + " " + str(s.get("name", "")) for s in steps]
verify = next(i for i, s in enumerate(steps) if "cosign verify" in str(s.get("run", "")))
pr = next(i for i, n in enumerate(names) if "create-pull-request" in n)
assert verify < pr, (verify, pr)
' "$WORKFLOW"
}

test_verify_pins_workflow_identity() {
    step_text promote-production | grep -q 'certificate-identity-regexp' \
        && step_text promote-production | grep -q 'token.actions.githubusercontent.com'
}

check "workflow has id-token and attestations permissions" test_workflow_has_oidc_and_attestation_permissions
check "build job signs the pushed digest" test_build_signs_pushed_digest
check "signing targets the build step digest" test_signing_uses_built_digest
check "build job attaches SBOM attestation" test_build_generates_sbom_attestation
check "build job attests provenance" test_build_attests_provenance
check "promotion verifies signature before opening PR" test_promotion_verifies_signature_before_pr
check "verification pins workflow OIDC identity" test_verify_pins_workflow_identity

if [[ $failures -gt 0 ]]; then
    echo "$failures check(s) failed"
    exit 1
fi
echo "All image signing workflow checks passed"
