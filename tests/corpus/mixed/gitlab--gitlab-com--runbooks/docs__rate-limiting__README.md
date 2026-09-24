<!-- MARKER: do not edit this section directly. Edit services/service-catalog.yml then run scripts/generate-docs -->

# Rate Limiting Service

* [Service Overview](https://dashboards.gitlab.net/d/rate-limiting-main/rate-limiting-overview)
* **Alerts**: <https://alerts.gitlab.net/#/alerts?filter=%7Btype%3D%22rate-limiting%22%2C%20tier%3D%22sv%22%7D>
* **Label**: gitlab-com/gl-infra/production~"Service::RateLimiting"


<!-- END_MARKER -->

## Overview of Rate Limits for <https://gitlab.com>

The handbook is the source of truth for [Rate Limiting information](https://handbook.gitlab.com/handbook/engineering/infrastructure-platforms/rate-limiting/).

If you are looking for information about requesting a rate limit bypass for GitLab.com, please see the
[Rate Limit bypass policy](https://handbook.gitlab.com/handbook/engineering/infrastructure-platforms/rate-limiting/bypass-policy/).

This section of documentation is targeted at SREs working in the production environment.

## How-Tos

Even with rate limiting in place, it's possible you need to explicitly block an IP or User sending exceptionally high volumes of traffic.

Alternatively, customers may request a bypass and if this is approved, you will need to follow the steps in this doc to temporarily implement an approved bypass.

### Block an IP or Path

This can be done using Cloudflare WAF for HTTP traffic. Cloudflare is the **preferred first option for blocking HTTP/S traffic** — see the [Cloudflare blocking and managing traffic runbook](../cloudflare/managing-traffic.md#when-to-block-and-how-to-block) for the decision matrix, scenarios, and the canonical Terraform workflow.

In case of an incident that requires immediate intervention, this can be done in the [Cloudflare dashboard](https://dash.cloudflare.com/852e9d53d0f8adbd9205389356f2303d/gitlab.com/security/waf/custom-rules) and backported in Terraform. If a rule is not backported then it will be deleted on the next Terraform Apply.

* To **block** an IP using a Custom Rule, add it to [this file](https://ops.gitlab.net/gitlab-com/gl-infra/config-mgmt/-/blob/main/environments/gprd/cloudflare-custom-rules.tf).
* To **throttle** an IP using a Rate Limit rule, add it to this [file](https://ops.gitlab.net/gitlab-com/gl-infra/config-mgmt/-/blob/main/environments/gprd/cloudflare-rate-limits-waf-and-rules.tf)

Please leave the appropriate incident or issue number, and information about when the rule can be deleted, in the rule description.

Work with an IMOC or peer to validate the change is reasonable and correct.

These will typically be temporary; anything permanent needs more careful discussion.

### Block a User or Project

GitLab team members with Administrator access can log in using their admin credentials, find the user's profile and block them. Follow steps for [Contacting users about GitLab incidents or changes](https://handbook.gitlab.com/handbook/support/internal-support/#contacting-users-about-gitlab-incidents-or-changes).

You can also follow the steps to [block a project causing high load](../uncategorized/block-high-load-project.md).

### Allow an IP or User

See the [Implementing Bypasses](#implementing-bypasses) section below.

### What Layer is a Rate Limit Happening?

Detailed instructions for this can be found in the [Rate Limiting Troubleshooting handbook page](https://handbook.gitlab.com/handbook/engineering/infrastructure-platforms/rate-limiting/troubleshooting/).

### Modifying a Rate Limit

If one endpoint (or a collection of endpoints) are being unduly rate-limited, we can consider increasing the limit for them.
How you proceed will depend on the urgency of the request:

**Is this urgent?**

**Yes:** Implement the increased rate-limit and set the bypass header in Cloudflare where appropriate.

**No:** Consider other options:

* Can the limit be modified in the Application (either RackAttack or ApplicationRateLimiter)?
* Creating a limit that takes into account user-identify (rather than just IP address) is preferable.

**Considerations**

* Gather evidence using logs and metrics to determine what is going on and why the limit might need adjusted.
* Document your findings in an issue, getting input from `@gitlab-org/production-engineering/managers`
* Verify whether our infrastructure can handle the proposed increase.
  * Consider the Database, Gitaly, and Redis, as well as frontend compute.
* Would setting the Bypass Header for specific requests (URL patterns or other identifiers) be sufficient?
* If it is agreed to proceed, raise a production change issue, linked to the earlier discussion issue, to execute the
change.
* Ensure [GitLab.com Specific Rate Limits](https://gitlab.com/gitlab-org/gitlab/-/tree/master/doc/user/gitlab_com/#gitlabcom-specific-rate-limits) is
updated to match the new values.

## Bypasses and Special Cases

[Published rate limits](https://docs.gitlab.com/user/gitlab_com/#gitlabcom-specific-rate-limits) apply to
all customers and users with no exceptions. Rate limiting bypasses are only allowed for specific cases.

We need special handling for various partners and other scenarios (e.g. excluding GitLab's internal services).
To permit this we have lists of IP addresses, termed `allowlist` that are permitted to bypass the rate limits.

Trusted IPs from customers/partners can be added to the allowlists, however we'd prefer to whittle this list _down_,
not add to it. The [Rate Limit bypass policy](https://handbook.gitlab.com/handbook/engineering/infrastructure-platforms/rate-limiting/bypass-policy/)
must be followed when considering adding to this list.

* **Cloudflare**
  * Custom rule bypass: [example](https://ops.gitlab.net/gitlab-com/gl-infra/config-mgmt/-/blob/main/environments/gprd/cloudflare-custom-rules.tf#L156) (confidential)

User-based bypasses are preferred over IP based, as IP addresses are a poor proxy for actual identity.
User IDs are much less fungible, and carry implications of paid groups/users and permanent identities of customers,
whereas there could be multiple users behind a single IP address and these can `rot` if they are no longer used by the
original user.

### Steps to follow before implementing a bypass

* Engage with the customer (via their TAM) and endeavour to find a way to achieve their goals without bypasses.
* May require development to enhance the API or webhooks (add more information so it can be pushed to the customer, rather than polled).
* In some cases, adding a couple of fields to a webhook can eliminate the need for many API calls.
* If implementing a bypass is unavoidable due to incident or temporary urgent customer need then follow the steps listed in the [bypass policy](https://handbook.gitlab.com/handbook/engineering/infrastructure-platforms/rate-limiting/bypass-policy/)

Customers with IPs present in the allow list can be assumed to have legacy grant and may have IPs added as necessary,
as long as the ask is reasonable (e.g. adding a few more where there are already many; questions should be asked if they ask
to add 100 when they currently have 2).

### Bypass headers

The `X-GitLab-RateLimit-Bypass` header is set to `0` by default. Any value set for this by the client request is overwritten by Cloudflare.

Requests from IPs with a bypass configured will have the `X-GitLab-RateLimit-Bypass` header set to 1, which RackAttack
interprets to mean these requests bypass the rate limits. Ideally we will remove this eventually, once the bypass list
is smaller (or gone), or we've ensured that our known users are below the new limits.

There are a few other special cases that also set `X-GitLab-RateLimit-Bypass` - these include certain paths, internal infrastructure addresses such as runner managers, or 3rd party vendors who have integrations with us.

All current bypasses for customers are implemented [here](https://ops.gitlab.net/gitlab-com/gl-infra/config-mgmt/-/blob/main/environments/gprd/cloudflare-custom-rules.tf).

### Tracking Bypasses

Link any bypasses created to [Customers Bypassing Rate-Limiting Epic](https://gitlab.com/groups/gitlab-com/gl-infra/-/epics/374) so that we can track it to completion.

These are _never_ permanent, they are only stepping stones to making the API better or otherwise enhancing the product to eliminate
the excessive traffic. In practice what we have found so far is issues like webhooks payloads lacking trivial details that
must then be scraped/polled from the API instead, and so on.

Anytime an IP is added to the allowlist, an issue for removing the IP should be [opened in the production engineering tracker](https://gitlab.com/gitlab-com/gl-infra/production-engineering/-/issues/new) cross-linking the original issue or incident where the IP was added and setting a due date for the IPs to be removed. In the case of allow-list requests, this is **at most** 2 weeks after the IP was added.

If a rate limit was disabled or relaxed as part of an incident mitigation, also follow the general process for [mitigations that disable a safety limit](../incidents/README.md#mitigations-that-disable-a-safety-limit) to ensure it has an explicit DRI, a re-evaluation deadline, and an `~"infradev"`-tracked follow-up.

### Implementing Bypasses

#### Cloudflare (IP-based)

Cloudflare is responsible for IP-based rate limiting and bypasses on GitLab.com. **Do not put IP addresses into HAProxy or RackAttack for allowlisting!**

To add a new entry to the allowlist:

* Create a new variable containing the customer's IP addresses in [this file](https://ops.gitlab.net/gitlab-com/gl-infra/config-mgmt/-/blob/main/environments/gprd/allowlists.tf)
  * Put a link to the rate limiting request issue in the comments so that we can easily attribute the IPs later.
* **Bypass Cloudflare Rate Limits**
  * Add a new custom rule using the variable in [this file](https://ops.gitlab.net/gitlab-com/gl-infra/config-mgmt/-/blob/main/environments/gprd/cloudflare-custom-rules.tf)
  * The custom rule will tell Cloudflare WAF to skip all rate limiting rules for the listed IPs, bypassing them.
* **Bypass RackAttack Rate Limits** (configured in Cloudflare)
  * Add a new transform rule using the variable in [this file](https://ops.gitlab.net/gitlab-com/gl-infra/config-mgmt/-/blob/main/environments/gprd/cloudflare-transform-rules.tf)
  * The transform rule will tell Cloudflare to apply the `X-GitLab-RateLimit-Bypass: 1` header for all IPs in the allowlist, bypassing the RackAttack rate limits.

#### Rails (RackAttack)

##### User-based

Per the [docs](https://docs.gitlab.com/administration/settings/user_and_ip_rate_limits/#allow-specific-users-to-bypass-authenticated-request-rate-limiting), we can designate specific user IDs as being able to bypass authenticated rate limits.

1. Update the Vault secret [here](https://vault.gitlab.net/ui/vault/secrets/shared/kv/env%2Fgprd%2Ffrontend%2Fuser-ratelimit) by appending the user ID to the list.
2. Bump the version of the secret [here](https://gitlab.com/gitlab-com/gl-infra/k8s-workloads/gitlab-com/-/blob/master/releases/gitlab-external-secrets/values/gprd.yaml.gotmpl#L165).
3. Finally, [use the new version](https://gitlab.com/gitlab-com/gl-infra/k8s-workloads/gitlab-com/-/blob/master/releases/gitlab/values/gprd.yaml.gotmpl#L386) of the secret in the Helm chart.

##### IP-based

We previously used to put a list of IPs to be allowlisted into the `/var/opt/gitlab/rack_attack_ip_whitelist/ip_whitelist` file where it would be read by the application. This method is no longer used; you should put the IPs into Cloudflare instead.

### Rack Attack Rate Limits Configuration Summary

| Name | Actor | Path Pattern | HTTP Method | Default Rate | GitLab.com Rate | Staging Rate | Pre Rate |
| --- | --- | --- | --- | --- | --- | --- | --- |
| throttle_unauthenticated_web | IP Address | Non-API paths | ALL | 100 requests per 60 seconds | 500 requests per 60 seconds | 500 requests per 60 seconds | 2000 requests per 10 seconds |
| throttle_authenticated_web | User | Non-API paths | ALL | 100 requests per 60 seconds | 1000 requests per 60 seconds | 2000 requests per 60 seconds | 100000 requests per 60 seconds |
| throttle_product_analytics_collector | Application | Product analytics endpoints | ALL | 100 requests per 60 seconds | 100 requests per 60 seconds | 100 requests per 60 seconds | 100 requests per 60 seconds |
| throttle_unauthenticated_protected_paths | IP Address | /users/password, /users/sign_in, /api/session, etc. | ALL | 10 requests per 60 seconds | 10 requests per 60 seconds | 10 requests per 60 seconds | 10 requests per 60 seconds |
| throttle_authenticated_protected_paths_api | User | /users/password, /users/sign_in, /api/session, etc. | ALL | 10 requests per 60 seconds | 10 requests per 60 seconds | 10 requests per 60 seconds | 10 requests per 60 seconds |
| throttle_authenticated_protected_paths_web | User | /users/password, /users/sign_in, /api/session, etc. | ALL | 10 requests per 60 seconds | 10 requests per 60 seconds | 10 requests per 60 seconds | 10 requests per 60 seconds |
| throttle_unauthenticated_get_protected_paths | IP Address | /users/password, /users/sign_in, /api/session, etc. | GET | 10 requests per 60 seconds | 10 requests per 60 seconds | 10 requests per 60 seconds | 10 requests per 60 seconds |
| throttle_authenticated_get_protected_paths_api | User | /users/password, /users/sign_in, /api/session, etc. | GET | 10 requests per 60 seconds | 10 requests per 60 seconds | 10 requests per 60 seconds | 10 requests per 60 seconds |
| throttle_authenticated_get_protected_paths_web | User | /users/password, /users/sign_in, /api/session, etc. | GET | 10 requests per 60 seconds | 10 requests per 60 seconds | 10 requests per 60 seconds | 10 requests per 60 seconds |
| throttle_authenticated_git_lfs | User | /info/lfs and Git LFS API paths | ALL | 1000 requests per 60 seconds | 1000 requests per 60 seconds | 1000 requests per 60 seconds | 1000 requests per 60 seconds |
| throttle_unauthenticated_git_http | IP Address | Git HTTP endpoints | ALL | 3600 requests per 3600 seconds | 500 requests per 60 seconds | 500 requests per 60 seconds | 2000 requests per 10 seconds |
| throttle_authenticated_api | User | /api/* paths | ALL | 7200 requests per 3600 seconds | 6000 requests per 60 seconds | 2000 requests per 60 seconds | 20000 requests per 10 seconds |
| throttle_unauthenticated_api | IP Address | /api/* paths | ALL | 3600 requests per 3600 seconds | 500 requests per 60 seconds | 800 requests per 60 seconds | 2000 requests per 10 seconds |
| throttle_raw_endpoint | Project/File Path | `/*/raw/*` paths | GET | 300 requests per minute | 300 requests per minute | - | - |
| throttle_authenticated_git_ssh | User | Git SSH protocols | ALL | 600 requests per minute | 600 per minute | - | - |
| git_basic_auth | IP Address | `git`, `jwt/auth/` | ALL | 10 requests per 60 seconds | 300 requests per minute | 300 requests per minute | disabled |
| virtual_registries_endpoints_api_limit | User | /api/v4/virtual_registries/packages/maven/* | ALL | 1000 requests per 15 seconds | 1000 requests per 15 seconds | 1000 per 15 seconds | 1000 requests per 15 seconds |
| throttle_unauthenticated_packages_api | IP Address | /api/v4/packages/* | ALL | 800 requests per 15 seconds | 800 requests per 15 seconds | 800 requests per 15 seconds | 800 requests per 15 seconds |
| throttle_authenticated_packages_api | User | /api/v4/packages/* | ALL | 1000 requests per 15 seconds | 1000 requests per 15 seconds | 1000 requests per 15 seconds | 1000 requests per 15 seconds |
| throttle_unauthenticated_files_api | IP Address | `/api/v4/projects/*/repository/files/*` | ALL | 125 requests per 15 seconds | 125 requests per 15 seconds | 125 requests per 15 seconds | 125 requests per 15 seconds |
| throttle_authenticated_files_api | User | `/api/v4/projects/*/repository/files/*` | ALL | 500 requests per 15 seconds | 500 requests per 15 seconds | 500 requests per 15 seconds | 500 requests per 15 seconds |
| throttle_unauthenticated_deprecated_api | IP Address | Deprecated API endpoints | ALL | 1800 requests per 3600 seconds | 3600 requests per 3600 seconds | 1800 requests per 3600 seconds | 3600 requests per 3600 seconds |
| throttle_authenticated_deprecated_api | User | Deprecated API endpoints | ALL | 3600 requests per 3600 seconds | 3600 requests per 1800 seconds | 3600 requests per 1800 seconds | 3600 requests per 1800 seconds |
| throttle_incident_management_notification_web | Project | `/projects/*/alert_management_alerts/notify.json` | POST | 3600 requests per 3600 seconds | 3600 requests per 3600 seconds | 3600 requests per 3600 seconds | 3600 requests per 3600 seconds |

## Application (RackAttack)

### Enable an Application Rate Limit in "Dry Run" mode

It is possible to enable RackAttack rate limiting rules in "Dry Run" mode
which can be utilised when introducing new rate limits
by setting the `GITLAB_THROTTLE_DRY_RUN` environment variable
[[source](https://docs.gitlab.com/administration/settings/user_and_ip_rate_limits/#try-out-throttling-settings-before-enforcing-them)].

For `GitLab.com` these environment variables are managed in k8s-workloads,
and set in the [extraEnv](https://gitlab.com/gitlab-com/gl-infra/k8s-workloads/gitlab-com/-/blob/donna/dry-run-authenticated-rate-limits/releases/gitlab/values/gprd.yaml.gotmpl#L359).

Once the `GITLAB_THROTTLE_DRY_RUN` environment variable is configured in production,
you can then turn the specified throttle on, for example `throttle_authenticated_web`.
If the new limit that is being introduced is hit,
you should see `event_type="track"` in the RackAttack metrics and logs.

After validating the rate limit threshold is behaving as expected,
you should remove the event name from the `GITLAB_THROTTLE_DRY_RUN` environment variable
which will allow the rate limit to start throttling requests.

* [Metrics: RackAttack events by event name and type](https://dashboards.gitlab.net/goto/XVO2kVvNg?orgId=1)

## Tier-aware throttles (labkit)

GitLab.com applies request limits by subscription plan (Free, Premium,
Ultimate) plus a plan-independent limit for unauthenticated traffic, 60 an
hour per IP. They are labkit rules on the `rack_request` limiter, not
RackAttack throttles, so nothing above about `GITLAB_THROTTLE_DRY_RUN`
applies to them. Design:
[Tier-aware throttles](https://internal.gitlab.com/handbook/engineering/architecture/design-documents/rate-limiting/tier-aware-throttles/).

Every rule name contains `_traffic_per_`, and each limit exists twice: a
`_log` rule that only counts, and a `:limit` rule that blocks. A feature
flag decides which one matches a request, so moving between observe and
enforce is a flag flip, effective on the next request, with no deploy.

| Flag | On its own |
| --- | --- |
| `rate_limiter_plan_limits_<free\|premium\|ultimate>_info` | that plan's `_log` rules match and count |
| `rate_limiter_plan_limits_<plan>_enforce` | that plan's `:limit` rules match and block |
| `rate_limiter_unauthenticated_limits_info` | the unauthenticated `_log` rule counts |
| `rate_limiter_unauthenticated_limits_enforce` | the unauthenticated `:limit` rule blocks, **only while `_info` is also on** |

The per-plan rows follow the design doc. The unauthenticated row follows the
merged code, which gates `_enforce` on `_info`. The plan flags may be brought
in line later.

All eight are `gitlab_com_derisk` flags, so they exist only on GitLab.com.

### Is a 429 tier-aware?

On the [detail dashboard](https://dashboards.gitlab.net/d/rate-limiting-detail/rate-limiting-detail),
the "Tier-aware" panels in the Labkit row break the rules out by name.
`result="log"` is a would-be rejection during observe mode; `result="block"`
is an enforcing rule deciding to reject. A tier-aware 429 carries the rule
name in its `RateLimit-Name` header, which is how you tell one from any
other 429.

No separate per-throttle counter for those 429s exists, and none is planned.
`result="block"` on these panels is that count.

### Rolling back

Turn off the `_enforce` flag for the affected plan. Its `:limit` rules stop
matching and the `_log` twins take over, so the limit keeps counting but
stops blocking. For the unauthenticated limit, turning off `_info` stops
both rules at once.

```plaintext
/chatops run feature set rate_limiter_plan_limits_free_enforce false
```

Two things to know when flipping:

* Each rule has its own Redis counter, keyed by rule name. Switching from
  the `_log` rule to the `:limit` rule (or back) starts from zero, so every
  client gets a fresh allowance at the flip. Measured on 2026-09-07: 40
  requests in log mode, then enforcement on, then 60 more before the first
  block, against a 60-an-hour limit.
* A `_log` rule never blocks, but a whole-check fail-open (see the Alerts
  section) skips every rule after the failure, so the tier-aware counters
  can under-report. A `count_distinct` rule that skipped a missing
  identifier key skips only that rule, the check still enforces the rest,
  and it needs its own triage. The check-error panel next to the tier-aware
  panels counts both and cannot tell them apart.

## Alerts

The `rate-limiting` service in the
[metrics catalog](../../metrics-catalog/services/rate-limiting.jsonnet)
generates SLO alerts for the application-level rate limiter driven by the
[`Labkit::RateLimit` library](https://gitlab.com/gitlab-org/ruby/gems/labkit-ruby/-/blob/master/lib/labkit/rate_limit/README.md)
(the migration target for RackAttack and ApplicationRateLimiter call sites).
The library **fails open**: a rate-limiter error never blocks a request, so
these alerts signal lost rate-limiting coverage, not user-facing request
failures.

### Metrics behind the alerts

* `gitlab_labkit_rate_limiter_checks_total{rate_limiter, action, matched, error}` —
  one increment per `check` call, including checks that failed open.
  `action` is the caller-facing decision (`allow` or `block`); `error="true"`
  marks a check that encountered an error (Redis unavailable, or a
  `count_distinct` rule skipped over a missing identifier key).
* `gitlab_labkit_rate_limiter_rule_evaluations_total{rate_limiter, rule, action, result}` —
  one increment per evaluated rule; powers the per-rule breakdowns on the
  [detail dashboard](https://dashboards.gitlab.net/d/rate-limiting-detail/rate-limiting-detail).

### What the alerts mean

* **`RateLimitingServiceRateLimiterChecksErrorSLOViolation`** — the fraction
  of checks encountering an error
  (`checks_total{error="true"} / checks_total`) burned through the error
  budget. Two distinct causes set `error="true"`:
  * **Whole-check fail-open** — usually the `redis-cluster-ratelimiting`
    service being unhealthy. Rate limits are effectively not enforced for
    the affected traffic. Check that service's alerts and dashboards first;
    the SLI declares the dependency, so a firing Redis alert is the
    upstream cause.
  * **A `count_distinct` rule skipped over a missing identifier key** — a
    caller configuration bug that fires with **healthy Redis** (no
    inhibiting Redis alert; the check still completes and enforces other
    rules). Search the gitlab-rails logs for
    `rate_limit_missing_count_distinct` — the log line carries the rule
    name.
* **`RateLimitingServiceRateLimiterChecksTrafficCessation`** /
  **`...TrafficAbsent`** — the check rate dropped to zero or the metric
  disappeared. Rate-limit middleware is likely not running or not emitting
  metrics: check for recent gitlab-rails deployments or configuration
  changes that could have disabled the labkit middleware or its metrics.

### Triage

1. Open the
   [rate-limiting detail dashboard](https://dashboards.gitlab.net/d/rate-limiting-detail/rate-limiting-detail)
   — the checks panel shows per-limiter request/block rates, the rule
   evaluations panel shows which rule is deciding.
2. Correlate with the `redis-cluster-ratelimiting` service overview if the
   alert is error-ratio based.
3. Rate-limit log lines are emitted in the calling service's structured logs
   (gitlab-rails application logs), with `name` (limiter), `rule`, and
   `error_type: rate_limit_error` fields for fail-open events.
