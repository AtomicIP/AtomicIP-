#!/usr/bin/env bash

################################################################################
# Tests for #1256: NetworkPolicy on deploy/k8s/base
#
# Verifies that the base manifests contain a default-deny ingress policy for
# the namespace, that the api-server is reachable only from the ingress
# controller and the monitoring namespace, and that kustomize renders them.
#
# Run with: ./scripts/test_k8s_networkpolicy.sh
################################################################################

set -u

BASE_DIR="$(cd "$(dirname "$0")/.." && pwd)/deploy/k8s/base"
POLICY_FILE="$BASE_DIR/networkpolicy.yaml"
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

# Print the policies as JSON so assertions can be made with python.
policies_json() {
    python3 -c '
import json, sys, yaml
docs = [d for d in yaml.safe_load_all(open(sys.argv[1])) if d]
print(json.dumps(docs))
' "$POLICY_FILE"
}

assert_py() {
    python3 -c "
import json, sys
docs = json.load(sys.stdin)
by_name = {d['metadata']['name']: d for d in docs}
$1
" <<< "$(policies_json)"
}

test_file_exists() {
    [[ -f "$POLICY_FILE" ]]
}

test_default_deny_selects_all_pods() {
    policies_json | assert_py "
d = by_name['atomicip-default-deny-ingress']
assert d['kind'] == 'NetworkPolicy'
assert d['spec']['podSelector'] == {}
assert d['spec']['policyTypes'] == ['Ingress']
assert 'ingress' not in d['spec'] or d['spec']['ingress'] == []
"
}

test_api_allow_only_from_ingress_and_monitoring() {
    policies_json | assert_py "
d = by_name['atomicip-api-allow-ingress']
assert d['spec']['podSelector']['matchLabels'] == {'app': 'atomicip-api'}
rules = d['spec']['ingress']
sources = sorted(
    p['namespaceSelector']['matchLabels']['kubernetes.io/metadata.name']
    for r in rules for p in r['from']
)
assert sources == ['ingress-nginx', 'monitoring'], sources
ports = {r['from'][0]['namespaceSelector']['matchLabels']['kubernetes.io/metadata.name']: r['ports'][0]['port'] for r in rules}
assert ports == {'ingress-nginx': 'http', 'monitoring': 'metrics'}, ports
"
}

test_no_allow_all_rule() {
    policies_json | assert_py "
for d in docs:
    for r in d['spec'].get('ingress', []) or []:
        assert r.get('from'), 'rule without from would allow all sources: ' + d['metadata']['name']
"
}

test_kustomization_includes_policy() {
    grep -q "^  - networkpolicy.yaml$" "$BASE_DIR/kustomization.yaml"
}

test_kustomize_renders_policies() {
    if ! command -v kubectl &> /dev/null; then
        echo "      (kubectl not installed; skipping render check)"
        return 0
    fi
    # jobs/ loads scripts from outside its directory, so relax the restrictor.
    kubectl kustomize --load-restrictor=LoadRestrictionsNone "$BASE_DIR" 2>/dev/null | grep -q "kind: NetworkPolicy"
}

check "policy file exists" test_file_exists
check "default-deny ingress selects all pods" test_default_deny_selects_all_pods
check "api allows only ingress-nginx and monitoring" test_api_allow_only_from_ingress_and_monitoring
check "no rule allows all sources" test_no_allow_all_rule
check "kustomization includes networkpolicy.yaml" test_kustomization_includes_policy
check "kustomize renders NetworkPolicy" test_kustomize_renders_policies

if [[ $failures -gt 0 ]]; then
    echo "$failures check(s) failed"
    exit 1
fi
echo "All NetworkPolicy checks passed"
