# PatroniIsDown

## Overview

- This alert fires when the `patroni` process is no longer running on a Patroni node. Patroni manages the local PostgreSQL instance, so if Patroni is gone PostgreSQL is usually down on that host too, and the node can neither serve traffic nor participate in failover.
- The alert is **not** scoped to a single cluster. The expression matches on the process group name only, so it fires for any Patroni node in the environment: `patroni` (main), `patroni-ci`, `patroni-registry` and `patroni-sec`. Always read the `type` and `fqdn` labels first to establish which cluster and node are affected.
- Contributing factors include an OOM kill or crash of the Patroni process, a failed or partially-applied `chef-client` converge, a node being rebuilt or decommissioned while still being scraped, and a host that has become unresponsive.
- On receiving this alert, confirm whether the node is a leader or a replica, confirm whether the cluster still has a healthy leader, and then decide between restarting Patroni and removing the node from service.

## Services

- [Patroni (main) overview](https://dashboards.gitlab.net/d/patroni-main)
- [Patroni CI overview](https://dashboards.gitlab.net/d/patroni-ci-main)
- [Patroni Registry overview](https://dashboards.gitlab.net/d/patroni-registry-main)
- [Patroni Sec overview](https://dashboards.gitlab.net/d/patroni-sec-main)
- [Patroni service runbook](../README.md)
- [Patroni cluster management](../patroni-management.md)
- Team that owns the service: [Database Automation](https://handbook.gitlab.com/handbook/engineering/data-engineering/database-excellence/database-automation)

The service overview dashboards above each contain a collapsed **patroni process stats** row, which is the panel that corresponds directly to this alert's metric.

## Metrics

- The alert is based on `namedprocess_namegroup_num_procs`, exported by `process_exporter`. It is a gauge: the number of running processes in a named process group. The `patroni` group is configured per node under `default_attributes.process_exporter.config.process_names` in the Chef role.
- The threshold is a straight `== 0` — zero `patroni` processes — held `for: 5m`. The 5 minute window is there to ride out an ordinary Patroni restart (a `gitlab-patronictl restart` or a converge-triggered restart completes well inside it), so a firing alert means the process has genuinely not come back.
- Under normal conditions this series sits at a small non-zero value on every Patroni node and the alert expression returns no results at all. A node dropping to `0` with the rest of the cluster unaffected points at that host; several nodes in one cluster dropping together points at a cluster-wide or fleet-wide cause such as a bad converge.
- Note that if the node stops being scraped entirely the series disappears rather than going to zero, so this alert goes quiet and [PatroniScrapeFailures](PatroniScrapeFailures.md) fires instead. Treat the two as a pair.

## Alert Behavior

- Severity `s1`, pages via PagerDuty, routed to the `database_automation` team.
- This alert is expected to be rare. It is not maintenance-aware: it will fire for a node that is intentionally stopped, so create a silence in [Alertmanager](https://alerts.gitlab.net/#/silences/new) **before** stopping Patroni, rebuilding a replica, or decommissioning a cluster.
- Node rebuilds are a known false-positive path. The `pg-replica-rebuild` playbooks disable `chef-client` and stop Patroni while the node is still registered for scraping, which is enough to fire this alert. See [production-engineering#25699](https://gitlab.com/gitlab-com/gl-infra/production-engineering/-/work_items/25699) for a related case where the same playbooks left state behind.

## Severities

- Default to `s1` and downgrade only once you have established the blast radius.
- Who is impacted depends on the cluster and the node's role:
  - A **leader** down on `patroni` (main) or `patroni-ci` affects nearly all customers until failover completes.
  - A **replica** down reduces read capacity and removes failover headroom, but is usually not customer-facing on its own.
  - `patroni-registry` affects container registry users; `patroni-sec` affects security-feature workloads rather than the main application path.
- Check how many nodes in the cluster are affected, and whether the cluster still has a leader — a cluster with a healthy leader and one missing replica is materially less severe than a cluster with no leader.
- Review the [Incident Severity handbook page](https://handbook.gitlab.com/handbook/engineering/infrastructure-platforms/incident-management/#incident-severity) to confirm.

## Verification

Confirm the alert against the metric, then against the node itself.

- Which nodes are reporting zero Patroni processes:

```promql
namedprocess_namegroup_num_procs{groupname="patroni", env="gprd"} == 0
```

- The same series over time for the affected cluster, to see when it dropped and whether it is flapping:

```promql
namedprocess_namegroup_num_procs{groupname="patroni", env="gprd", type="<type from the alert>"}
```

- Cross-check that the node is still being scraped at all. If this returns `0` the node is unreachable and this is a host problem rather than a Patroni process problem:

```promql
up{fqdn="<fqdn from the alert>", env="gprd"}
```

- Then on the node:

```bash
ssh <fqdn from the alert>
sudo systemctl status patroni
sudo journalctl -u patroni --since '-30min'
sudo gitlab-patronictl list
```

`gitlab-patronictl list` can be run from any surviving member of the cluster and is the fastest way to see whether a leader is still present.

## Recent changes

- [Recent Patroni service change issues](https://gitlab.com/gitlab-com/gl-infra/production/-/issues/?sort=updated_desc&state=opened&or%5Blabel_name%5D%5B%5D=Service%3A%3APatroni&or%5Blabel_name%5D%5B%5D=Service%3A%3APatroniCI&or%5Blabel_name%5D%5B%5D=Service%3A%3APatroniRegistry&or%5Blabel_name%5D%5B%5D=Service%3A%3APatroniSec&first_page_size=20)
- Recent [chef-repo merge requests](https://gitlab.com/gitlab-com/gl-infra/chef-repo/-/merge_requests?scope=all&state=merged) and [config-mgmt merge requests](https://ops.gitlab.net/gitlab-com/gl-infra/config-mgmt/-/merge_requests?scope=all&state=merged) — a converge that changes the Patroni or PostgreSQL configuration is a common trigger for several nodes going down together.

## Troubleshooting

Work from cluster health inwards to the individual node.

1. Establish the cluster's state with `sudo gitlab-patronictl list` on a surviving member. If there is no `Leader`, that is the priority — the cluster is not accepting writes.
2. Establish whether the affected node is reachable. If SSH and `up{}` both fail, treat it as a host failure and follow [PatroniScrapeFailures](PatroniScrapeFailures.md).
3. If the node is reachable, check whether Patroni was killed or failed to start:

```bash
sudo journalctl -u patroni --since '-1h' --no-pager
sudo dmesg -T | grep -i 'killed process'
```

4. Check whether `chef-client` has been disabled on the node, which would indicate planned maintenance that should have been silenced. Chef runs are disabled with `chef-client-disable` and re-enabled with `chef-client-enable` (see [Disable Chef runs on a VM](../../fleet-management/config_management/disable-chef-runs-on-a-vm.md)), and the state is exported as `chef_client_enabled`:

```promql
chef_client_enabled{fqdn="<fqdn from the alert>", env="gprd"} == 0
```

5. Confirm the disk has not filled, which will stop both PostgreSQL and Patroni:

```bash
df -h /var/opt/gitlab
```

- [Helpful gitlab-patronictl commands](../patroni-management.md)

## Possible Resolutions

- If Patroni was killed but the host is otherwise healthy, restart it and watch it rejoin: `sudo systemctl start patroni` then `watch -n 30 'gitlab-patronictl list'`. Expect the member to move through `starting` into `streaming`.
- If the node was intentionally stopped for maintenance, silence the alert and record the silence on the change issue. Do not restart Patroni on a node that is mid-rebuild.
- If the cluster has lost its leader and will not elect one, follow the failover guidance in [Patroni cluster management](../patroni-management.md) and escalate — do not force a promotion without a second pair of eyes.
- If the host is unresponsive, remove it from service and rebuild rather than trying to recover Patroni in place.

## Dependencies

- **Consul** — Patroni uses Consul for leader election and service registration. A Consul agent problem on the node can prevent Patroni from starting or staying healthy. See [Consul commands for Patroni](../../consul/interaction.md#some-interesting-commands-for-patroni).
- **PostgreSQL** — a PostgreSQL instance that cannot start (corrupt state, full disk, bad configuration) will keep Patroni crash-looping.
- **Chef** — Patroni's configuration is managed by `chef-client`; a failed or partial converge can leave the service stopped or misconfigured.
- **`process_exporter`** — this alert's metric source. If `process_exporter` is running but its configuration no longer matches the Patroni process, the count can read zero while Patroni is in fact healthy. Verify with `systemctl status patroni` before acting.

## Escalation

- Slack channel: `#g_database_operations`
- Alert traffic for this team lands in `#s_database_excellence_alerts`
- For anything meeting the [infra-dev escalation criteria](https://handbook.gitlab.com/handbook/engineering/workflow/development-processes/infra-dev-escalation/process/), use `/devoncall <incident_url>` in Slack.

## Definitions

- [Alert definition](https://gitlab.com/gitlab-com/runbooks/-/blob/master/mimir-rules/gitlab-gprd/patroni/patroni.yml#L17) (`mimir-rules/gitlab-<env>/patroni/patroni.yml`)
- Tuning advice: the `for: 5m` window exists to absorb planned restarts. Shortening it will produce noise on every converge that restarts Patroni. Raising the `== 0` threshold is not meaningful — the correct fix for a noisy node is a silence, not a weaker condition.
- [Link to edit this playbook](PatroniIsDown.md)
- [Update the template used to format this playbook](https://gitlab.com/gitlab-com/runbooks/-/edit/master/docs/template-alert-playbook.md)

## Related Links

- [Related alerts](./)
- [PatroniScrapeFailures](PatroniScrapeFailures.md) — fires instead of this alert when the node stops being scraped altogether
- [PostgresSplitBrain](PostgresSplitBrain.md) — can follow a mishandled failover
- [Postgres runbook docs](../postgres.md)
