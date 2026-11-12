#!/bin/sh
# Fails when a T3 RPC name appears outside the T3 adapter (see AGENTS.md).
set -eu
cd "$(dirname "$0")/.."
pattern='orchestration\.(getThreadProjection|subscribeShell|dispatchCommand|launchThread|searchThreads)|scheduledTasks\.(subscribe|upsert|setEnabled|delete|runNow|listRuns)|preview\.list|projects\.mutate|secrets\.answerRequest|runtime-request\.respond|server\.(getConfig|refreshProviders)|vcs\.refreshStatus|pullRequests\.list|subscribeVcsStatus'
bad=0
for f in $(find gateway -name '*.rs' -not -path 'gateway/src/harness/adapters/t3/*'); do
  if grep -E -n -H -e "$pattern" "$f"; then
    bad=1
  fi
done
if [ "$bad" -ne 0 ]; then
  echo "T3 RPC names belong only in gateway/src/harness/adapters/t3/"
  exit 1
fi
