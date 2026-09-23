# What tests can be run?

[[_TOC_]]

## The two types of QA tests

First of all, the first thing to choose is whether you want to run orchestrated
tests (various Docker containers are spun up and tests are run against them,
also from a specific Docker container) or instance-level tests (tests are run
from your host machine against a live instance: a local GDK installation or a staging/production instance).

Ultimately, orchestrated tests run instance-level tests, the difference being
that these tests are run from a specific Docker container instead of from your
host machine.

## Orchestrated tests

Orchestrated tests are run with the `gitlab-qa` binary (from the
`gitlab-qa` gem), or in the `gitlab-qa` project, with the `exe/gitlab-qa` binary
(useful if you're working on the `gitlab-qa` project itself and want to test
your changes).

These tests spin up Docker containers specifically to run tests against them.
Orchestrated tests are usually used for features that involve external services
or complex setup (e.g. LDAP, Geo etc.), or for generic Omnibus checks (ensure
our Omnibus package works, can be updated / upgraded to EE etc.).

For more details on the internals, please read the
[How it works](./how_it_works.md) documentation.

## Supported GitLab environment variables

All environment variables used by GitLab QA should be defined in [`lib/gitlab/qa/runtime/env.rb`](https://gitlab.com/gitlab-org/gitlab-qa/-/blob/master/lib/gitlab/qa/runtime/env.rb).

|        Variable       | Default   | Description           | Required |
|-----------------------|-----------|-----------------------|----------|
| `GITLAB_USERNAME`       | `root`  | Username to use when signing into GitLab. | Yes|
| `GITLAB_PASSWORD`       | `5iveL!fe` | Password to use when signing into GitLab. | Yes|
| `GITLAB_QA_USERNAME_1`  |-  | Username available in environments where signup is disabled. | No|
| `GITLAB_QA_PASSWORD_1`  |-  | Password for `GITLAB_QA_USERNAME_1` available in environments where signup is disabled (e.g. staging.gitlab.com). | No|
| `GITLAB_LDAP_USERNAME`  |-  | LDAP username to use when signing into GitLab. | No|
| `GITLAB_LDAP_PASSWORD`  |-  | LDAP password to use when signing into GitLab. | No|
| `GITLAB_ADMIN_USERNAME` |-  | Admin username to use when adding a license. | No|
| `GITLAB_ADMIN_PASSWORD` |-  | Admin password to use when adding a license. | No|
| `GITLAB_SANDBOX_NAME`   | `gitlab-qa-sandbox` | The sandbox group name the test suite is going to use. | No|
| `GITLAB_QA_ACCESS_TOKEN`|-  | A valid personal access token with the `api` scope. This allows tests to use the API without having to create a new personal access token first. It is also used to check what version [deployed environments](https://gitlab.com/gitlab-org/quality/pipeline-common/-/blob/master/ci/dot-com/README.md#projects) are currently running. Existing tokens for each environment can be found in the shared 1Password vault. |No|
| `GITLAB_QA_ADMIN_ACCESS_TOKEN` |-  | A valid personal access token with the `api` scope from a user with admin access. Used for API access as an admin during tests. | No|
| `GITLAB_QA_CONTAINER_REGISTRY_ACCESS_TOKEN` | - | A valid personal access token with the `read_registry` scope. Used to [access the container registry on `registry.gitlab.com` when tests run in a CI job that _is not_ triggered via another pipeline](https://gitlab.com/gitlab-org/gitlab-qa/-/blob/364addb83e7b136ff0f9d8719ca9553d290aa9ab/lib/gitlab/qa/release.rb#L152). For example, if you manually run a [new Staging pipeline](https://ops.gitlab.net/gitlab-org/quality/staging/-/pipelines/new), this token will be used. | No |
| `EE_LICENSE` |-  | Enterprise Edition license. For Staging license to be applied successfully requires `CUSTOMER_PORTAL_URL` set. | No|
| `GITLAB_LICENSE_MODE` |- | If set to `test` gitlab-qa will then set customers portal address to Staging address. | No|
| `QA_EE_ACTIVATION_CODE` |-  | Cloud activation code to enable Enterprise Edition features. | No|
| `QA_ARTIFACTS_DIR` |`/tmp/gitlab-qa`| Path to a directory where artifacts (logs and screenshots) for failing tests will be saved. | No|
| `DOCKER_HOST` |`http://localhost`| Docker host to run tests against. | No|
| `QA_DOCKER_NETWORK` | `test` | Name of the Docker network that is created to allow connectivity between GitLab and other containers. | No |
| `WEBDRIVER_HEADLESS` |-  | When running locally, set to `false` to allow Chrome tests to be visible - watch your tests being run. | No|
| `CHROME_DISABLE_DEV_SHM` | `false` | Set to `true` to disable `/dev/shm` usage in Chrome on Linux. | No|
| `QA_ADDITIONAL_REPOSITORY_STORAGE` |-  | The name of additional, non-default storage to be used with tests tagged `repository_storage`, run via the `Test::Instance::RepositoryStorage` scenario.  Note: Admin access is required to change repository storage. | No|
| `QA_PRAEFECT_REPOSITORY_STORAGE` |-  | The name of repository storage using Praefect. Note: Admin access is required to change repository storage. | No|
| `QA_COOKIES` |-  | Optionally set to "cookie1=value;cookie2=value" in order to add a cookie to every request. This can be used to set the canary cookie by setting it to "gitlab_canary=true", or to use a different cell e.g. "_gitlab_session=cell-2-please". | No|
| `QA_LOG_LEVEL` | `info` | Logging level to use for gitlab-qa output and qa tests output | No|
| `QA_LOG_PATH` | `QA_ARTIFACTS_DIR` | Path to output debug logging to. | No|
| `QA_CAN_TEST_GIT_PROTOCOL_V2` | `true` | Set to `false` to skip tests that require Git protocol v2 if your environment doesn't support it. | No|
| `QA_CAN_TEST_ADMIN_FEATURES` | `true` | Set to `false` to skip tests that require admin access. | No|
| `QA_CAN_TEST_PRAEFECT` | `true` | Set to `false` to skip tests that require Praefect to be running. | No|
| `QA_RETRY_FAILED_SPECS` |-  | Set to `true` to retry failed specs after initial run finishes. | No|
| `QA_SIMULATE_SLOW_CONNECTION` |-  | Set to `true` to configure Chrome's network settings to simulate a slow connection. | No|
| `QA_SLOW_CONNECTION_LATENCY_MS` | `2000` | The additional latency (in ms) of the simulated slow connection. | No|
| `QA_SLOW_CONNECTION_THROUGHPUT_KBPS` | `32` | The maximum throughput (in kbps) of the simulated slow connection. | No|
| `QA_SKIP_PULL` | `false` | Set to `true` to skip pulling docker images (e.g., to use one you built locally). | No|
| `QA_GENERATE_ALLURE_REPORT` | `false` | When running on CI, set to `true` to generate allure reports | No|
| `QA_EXPORT_TEST_METRICS` | `true` | When running on CI, set to `true` to export test metrics to InfluxDB | No|
| `QA_INFLUXDB_URL` |- | InfluxDB URL for test metrics reporting | No|
| `QA_INFLUXDB_TOKEN` |- | InfluxDB token for test metrics reporting | No|
| `QA_RUN_TYPE` |- | QA run type like `staging-full`, `canary`, `production` etc. Used in test metrics reporting | No|
| `QA_VALIDATE_RESOURCE_REUSE` | `false` | Set to `true` to [validate resource reuse](https://docs.gitlab.com/ee/development/testing_guide/end_to_end/resources.html#reusable-resources) after a test suite | No |
| `QA_USE_PUBLIC_IP_API` | `false` | When performing Instance tests against a remote/pre-existing instance, use an API to detect the public API for requests coming from gitlab-qa.  Used by tests that exercise IP-address restrictions and similar | No |
| `QA_GITHUB_USERNAME` |-  | Username for authenticating with GitHub. | No|
| `QA_GITHUB_PASSWORD` |-  | Password for authenticating with GitHub. | No|
| `GITLAB_QA_LOOP_RUNNER_MINUTES` | `1` | Minutes to run and repeat a spec while using the '--loop' option; default value is 1 minute. | No|
| `CI_SERVER_PERSONAL_ACCESS_TOKEN` |-  | Personal access token of the server that is running the CI pipeline. This is used for pulling CI_RUNNER information in certain tests. | No|
| `GEO_MAX_FILE_REPLICATION_TIME` | `120` | Maximum time that a test will wait for a replicated file to appear on a Geo secondary node. | No|
| `GEO_MAX_DB_REPLICATION_TIME` | `120` | Maximum time that a test will wait for database data to appear on a Geo secondary node. | No|
| `JIRA_ADMIN_USERNAME` |-  | Username for authenticating with Jira server as admin. | No|
| `JIRA_ADMIN_PASSWORD` |-  | Password for authenticating with Jira server as admin. | No|
| `CACHE_NAMESPACE_NAME` | `true` | Cache namespace name for groups. | No|
| `DEPLOY_VERSION` |- | The version of GitLab being tested against. | No|
| `GITLAB_QA_USER_AGENT` |- | The browser user-agent to use instead of the default Chrome user-agent. When set to the appropriate value (stored in 1Password), this allows tests to bypass certain login challenges (e.g., reCAPTCHA and ArkoseLabs). | No|
| `GEO_FAILOVER` | `false` | Set to `true` when a Geo secondary site has been promoted to a Geo primary site. | No|
| `COLORIZED_LOGS` | `false` | Colors GitLab QA and test logs to improve readability | No|
| `QA_DOCKER_ADD_HOSTS` |- | Comma separated list of hosts to add to /etc/hosts in docker container | No|
| `FIPS` |- | Set to `1` or `true` to indicate that the test is running under FIPS mode | No|
| `JH_ENV` | `false` | Set to `true` to indicate tests or scenarios are running under JH env | No |
| `QA_GITHUB_OAUTH_APP_ID` | - | Client ID for GitHub OAuth app. See https://docs.gitlab.com/ce/integration/github.html for steps to generate this token. | No |
| `QA_GITHUB_OAUTH_APP_SECRET` | - | Client Secret for GitHub OAuth app. See https://docs.gitlab.com/ce/integration/github.html for steps to generate this token. | No |
| `QA_1P_EMAIL` | - | Email address for authenticating into 1Password. | No |
| `QA_1P_PASSWORD` | - | Password for authenticating into 1Password. | No |
| `QA_1P_SECRET` | - | Secret for authenticating into 1Password. | No |
| `QA_1P_GITHUB_UUID` | - | UUID for gitlab-qa-github item in gitlab-qa-totp 1Password vault. | No |
| `RELEASE` | - | The release image to use for an orchestrated GitLab instance. | No |
| `RELEASE_REGISTRY_URL` | - | The registry url to fetch the release image for an orchestrated GitLab. | No |
| `RELEASE_REGISTRY_USERNAME` | - | The username to log in to the registry for pulling the release image for orchestrated GitLab. | No |
| `RELEASE_REGISTRY_PASSWORD` | - | The password to log in to the registry for pulling the release image for orchestrated GitLab. | No |
| `WORKSPACES_OAUTH_APP_ID` | - | Client ID for Gitlab-workspaces-proxy OAuth app used for the authentication and authorization of the workspaces running in the cluster. See https://gitlab.com/gitlab-org/remote-development/gitlab-workspaces-proxy for instructions. | No |
| `WORKSPACES_OAUTH_APP_SECRET` | - | Client Secret for Gitlab-workspaces-proxy OAuth app used for the authentication and authorization of the workspaces running in the cluster. See https://gitlab.com/gitlab-org/remote-development/gitlab-workspaces-proxy for instructions. | No |
| `WORKSPACES_OAUTH_SIGNING_KEY` | - | Client Signing key for Gitlab-workspaces-proxy OAuth app used for the authentication and authorization of the workspaces running in the cluster. | No |
| `WORKSPACES_PROXY_DOMAIN` | - | The domain on which gitlab-workspaces-proxy will listen on and it is used to create workspaces url. | No |
| `WORKSPACES_DOMAIN_CERT` | - | The fullchain.pem SSL Certificates for the `WORKSPACES_PROXY_DOMAIN`. See https://gitlab.com/gitlab-org/remote-development/gitlab-workspaces-proxy for instructions. | No |
| `WORKSPACES_DOMAIN_KEY` | - | The privkey.pem SSL Certificates for the `WORKSPACES_PROXY_DOMAIN`. See https://gitlab.com/gitlab-org/remote-development/gitlab-workspaces-proxy for instructions. | No |
| `WORKSPACES_WILDCARD_CERT` | - | The fullchain.pem SSL Certificates for the Wildcard `WORKSPACES_PROXY_DOMAIN`. See https://gitlab.com/gitlab-org/remote-development/gitlab-workspaces-proxy for instructions.| No |
| `WORKSPACES_WILDCARD_KEY` | - | The privkey.pem SSL Certificates for the Wildcard `WORKSPACES_PROXY_DOMAIN`. See https://gitlab.com/gitlab-org/remote-development/gitlab-workspaces-proxy for instructions. | No |
| `QA_GITLAB_HOSTNAME` | `"gitlab-#{edition}-#{SecureRandom.hex(4)}"` | A host name for the GitLab instance setup with Test::Instance::Image. | No |
| `QA_GITLAB_USE_TLS` | false | Specify if GitLab instance setup with Test::Instance::Image should use TLS | No |

## [Supported Remote Grid environment variables](./running_against_remote_grid.md)

## Running tests with a feature flag enabled

It is possible to enable or disable a feature flag before running tests.
To test a GitLab image with a feature flag enabled, run this command:

```shell
$ gitlab-qa Test::Instance::Image gitlab/gitlab-ee:12.4.0-ee.0 --enable-feature feature_flag_name
```

To run a test with feature flag disabled, run this command:

```shell
$ gitlab-qa Test::Instance::Image gitlab/gitlab-ee:12.4.0-ee.0 --disable-feature feature_flag_name
```

You can also test a GitLab image multiple times with different feature flag settings:

```shell
$ gitlab-qa Test::Instance::Image gitlab/gitlab-ee:12.4.0-ee.0 --disable-feature feature_flag_name --enable-feature feature_flag_name
```

This will first disable `feature_flag_name` flag and run the tests and then enable it and run the tests again.

You can pass any number of feature flag settings. The tests will run once for each setting.

See the [QA framework documentation](https://gitlab.com/gitlab-org/gitlab/-/blob/master/qa/README.md#running-tests-with-a-feature-flag-enabled-or-disabled)
for information on running the tests with different feature flag settings from the QA framework.

## Running tests with multiple feature flags set

The options above allow you to enable or disable a single feature flag at a time. However, if you want to set more than
one feature flag at the same time you'll need to use `--set-feature-flags` instead.

The desired state must be set individually for each feature flag in a comma-separated list. For example to disable a feature flag
named `feature-one` and enable another named `feature-two`, use the following parameters:

```shell
--set-feature-flags feature-one=disable,feature-two=enable
```

Those parameters will instruct GitLab QA to set both feature flags before running the suite of tests.

## Specifying the GitLab version

In each of the examples below, it is possible to test a specific version of GitLab
by providing the full image name, or an abbreviation followed by the image tag.

For example, to test GitLab version `12.4.0-ee`, the image tag is [`12.4.0-ee.0`](https://hub.docker.com/layers/gitlab/gitlab-ee/12.4.0-ee.0/images/sha256-65df19d8abbb0debdccb64bfe96871563806098cd84da0b818ae8cfdd928b9aa)
and so you could run the tests with the command:

```shell
$ gitlab-qa Test::Instance::Image gitlab/gitlab-ee:12.4.0-ee.0
```

Or with the command:

```shell
$ gitlab-qa Test::Instance::Image EE:12.4.0-ee.0
```

If you only provide the abbreviation, it will run the tests against the latest nightly image.

For example, the following command would use the image named `gitlab/gitlab-ee:nightly`

```shell
$ gitlab-qa Test::Instance::Image EE
```

To run EE tests, the `EE_LICENSE` environment variable needs to be set:

`$ export EE_LICENSE=$(cat /path/to/GitLab.gitlab_license)`

## Specifying the GitLab QA image to use

By default, `gitlab-qa` infers the QA image to use based on the GitLab image.
For instance, if you run the following:

```shell
$ gitlab-qa Test::Instance::Image gitlab/gitlab-ee:12.4.0-ee.0
```

Then, `gitlab-qa` would infer `gitlab/gitlab-ee-qa:12.4.0-ee.0` as the QA image
based on the GitLab image (note the `-qa` suffix in the image name).

In some cases, you'll want to use a specific QA image instead of letting
`gitlab-qa` infer the QA image name from the GitLab image. Such cases can be
when you're doing local debugging/testing and you want to control the QA image
name, or in the CI where the QA image might be built by a project (e.g.
`gitlab-org/gitlab`, and the GitLab image might be built by another project
(e.g. `gitlab-org/omnibus-gitlab-mirror`)).

To specify the QA image to use, pass the `--qa-image QA_IMAGE` option,
as follows:

```shell
$ gitlab-qa Test::Instance::Image --qa-image registry.gitlab.com/gitlab-org/gitlab/gitlab-ee-qa:branch-name EE
```

Additionally, setting the `$QA_IMAGE` environment variable achieve the same result,
without needing to pass the `--qa-image` option:

```shell
$ export QA_IMAGE="registry.gitlab.com/gitlab-org/gitlab/gitlab-ee-qa:branch-name"
$ gitlab-qa Test::Instance::Image EE
```

## Running a specific test (or set of tests)

In most of the scenarios listed below, if you don't want to run all the tests
it's possible to specify one or more tests. The framework uses RSpec, so tests can be
specified as you would when using RSpec.

For example, the following would run `create_merge_request_spec.rb`:

```shell
$ gitlab-qa Test::Instance::Image EE -- qa/specs/features/browser_ui/3_create/merge_request/create_merge_request_spec.rb
```

While the following would run all Create UI tests:

```shell
$ gitlab-qa Test::Instance::Image EE -- qa/specs/features/browser_ui/3_create
```

And the following would run all Create API tests as well as UI tests:

```shell
$ gitlab-qa Test::Instance::Image EE -- qa/specs/features/browser_ui/3_create qa/specs/features/api/3_create
```

## Examples

### `Test::Instance::Image CE|EE|<full image address>`

This tests that a GitLab Docker container works as expected by running
instance-level tests against it.

To run tests against the GitLab containers, a GitLab QA (`gitlab/gitlab-qa`)
container is spun up and tests are run from it by running the `Test::Instance`
scenario (located under
[`gitlab-org/gitlab/qa/qa/scenario/test/instance.rb`][test-instance] in the
GitLab project).

Example:

```shell
$ gitlab-qa Test::Instance::Image CE
```

### `Test::Omnibus::Image CE|EE|<full image address>`

This tests that a GitLab Docker container can start without any error.

This spins up a GitLab Docker container based on the given edition or image:

- `gitlab/gitlab-ce:nightly` for `CE`
- `gitlab/gitlab-ee:nightly` for `EE`
- the given custom image for `<full image address>`

Example:

```shell
$ gitlab-qa Test::Omnibus::Image CE
```

### `Test::Omnibus::Upgrade CE|<full image address>`

This tests that:

- the GitLab Docker container works as expected by running instance-level tests
  against it (see `Test::Instance::Image` above)
- it can be upgraded to a corresponding EE container
- the new GitLab container still works as expected by running
  `Test::Instance::Image` against it

Example:

```shell
# Upgrade from gitlab/gitlab-ce:nightly to gitlab/gitlab-ee:nightly
$ gitlab-qa Test::Omnibus::Upgrade CE

# Upgrade from gitlab/gitlab-ce:my-custom-tag to gitlab/gitlab-ee:my-custom-tag
$ gitlab-qa Test::Omnibus::Upgrade gitlab/gitlab-ce:my-custom-tag
```

### `Test::Omnibus::UpdateFromPrevious <full image address> <current_version> <major|minor|patch> <from_edition>`

Scenario verifies upgrade from previous (major|minor|patch) version to current release.

- Deploys previous (major|minor|patch) version and runs Health Check test suite (Smoke test suite if below 17.1.0)
  to populate data in database before upgrade
- Generates upgrade path following current GitLab recommendations.
  The upgrade path is resolved in this order:
  1. `QA_UPGRADE_PATH_FILE` env var — explicit path to a local YAML file.
  2. `$CI_PROJECT_DIR/config/upgrade_path.yml` — set automatically in GitLab CI when running inside a `gitlab-org/gitlab` pipeline.
  3. `../config/upgrade_path.yml` relative to the working directory — covers running from the `qa/` subdirectory of a gitlab checkout.
  4. Remote fetch from [`gitlab.com/gitlab-org/gitlab`](https://gitlab.com/gitlab-org/gitlab/-/raw/master/config/upgrade_path.yml) (fallback when no local file is found).
- Gradually upgrades GitLab instances to `<current_version>`
  associated with provided `<full image address>` and runs tests against it

Example:

```shell
# Minor upgrade - will perform upgrade 15.5.x -> gitlab-ee:dev-tag
$ gitlab-qa Test::Omnibus::UpdateFromPrevious gitlab-ee:dev-tag 15.6.0-pre minor

# Major upgrade - will perform upgrades 14.10.x -> 15.0.x -> 15.4.x -> gitlab-ee:dev-tag (15.6.0-pre)
$ gitlab-qa Test::Omnibus::UpdateFromPrevious gitlab-ee:dev-tag 15.6.0-pre major

# Patch upgrade - will perform upgrade from 17.8.1 -> gitlab-ee:dev-tag (17.8.2-pre)
$ gitlab-qa Test::Omnibus::UpdateFromPrevious gitlab-ee:dev-tag 17.8.2-pre patch

# Internal patch upgrade - will perform upgrade from dev.gitlab.org:5005/gitlab/omnibus-gitlab/gitlab-ee:17.8.1-internal0-0 -> gitlab-ee:dev-tag (17.8.2-pre)
$ gitlab-qa Test::Omnibus::UpdateFromPrevious gitlab-ee:dev-tag 17.8.2-pre internal_patch
```

### `Test::Omnibus::UpdateToNext <full image address> <current_version> <from_edition>`

Scenario verifies upgrade from current development version to next available stable release.

- Deploys development version and runs Health Check test suite to populate data in database (Smoke test suite if below 17.1.0)
- Finds next available stable version
- Upgrades GitLab instance from development version to next stable and runs tests against it
- Skips if either current version or next version is not yet released

Note: `from_edition` setting is optional, can be set to `ee` or `ce`.

Example:

```shell
# Will perform upgrade from gitlab-ee:dev-tag (17.8.2-pre) -> 17.9.2
$ gitlab-qa Test::Omnibus::UpdateToNext gitlab-ee:dev-tag 17.8.2-pre
```

### `Test::Integration::Geo EE|<full image address>`

This tests that Geo UI proxying is working as expected.

The scenario spins up primary and secondary GitLab Geo nodes and
can be used to verify that web requests to secondary Geo site return
data that is present on the primary.

See https://docs.gitlab.com/ee/administration/geo/secondary_proxy

To run tests against the GitLab containers, a GitLab QA (`gitlab/gitlab-qa`)
container is spun up and tests are run from it by running the
`QA::EE::Scenario::Test::Geo` scenario (located under
[`gitlab-org/gitlab-ee@qa/qa/ee/scenario/test/geo.rb`][test-geo] in the GitLab
EE project).

**Required environment variables:**

- `EE_LICENSE`: A valid EE license.

Example:

```shell
$ export EE_LICENSE=$(cat /path/to/Geo.gitlab_license)
$ gitlab-qa Test::Integration::Geo EE
```

[test-cvs]: ...

### `Test::Integration::ContinuousVulnerabilityScanning EE|<full image address>`

This tests [Continuous Vulnerability Scanning](https://docs.gitlab.com/ee/user/application_security/continuous_vulnerability_scanning/)
which is functionality to allow updated vulnerabilities to be downloaded and shown for
relevant software dependencies.

It is designed to run against a particular end to end spec as per the example.

It is EE functionality and requires a license to be set.

**Required environment variables:**

- `EE_LICENSE`: A valid EE license.

Example:

```shell
$ export EE_LICENSE=$(cat /path/to/gitlab_license)
$ export GITLAB_LICENSE_MODE=test
$ gitlab-qa Test::Integration::ContinuousVulnerabilityScanning EE
````

[test-geo]: https://gitlab.com/gitlab-org/gitlab-ee/blob/master/qa/qa/ee/scenario/test/geo.rb

### `Test::Integration::GitalyCluster CE|EE|<full image address>`

This tests [Gitaly Cluster](https://docs.gitlab.com/ee/administration/gitaly/praefect.html),
a clustered configuration of the Gitaly repository storage service.

The scenario configures and starts several docker containers to
represent the [recommended minimum configuration](https://docs.gitlab.com/ee/administration/gitaly/praefect.html#requirements-for-configuring-a-gitaly-cluster)
of a Gitaly Cluster.

To run tests against the GitLab container, a GitLab QA (`gitlab/gitlab-qa`)
container is spun up and tests are run from it by running the
`Test::Instance::All` scenario with the `:gitaly_cluster` tag.

Example:

```shell
$ gitlab-qa Test::Integration::GitalyCluster EE
```

### `Test::Integration::GitlabPages CE|EE|<full image address>`

This tests that a GitLab Instance works as expected with [GitLab Pages](https://docs.gitlab.com/ee/user/project/pages/) enabled.

The scenario configures the instance by setting up these instructions [here](https://gitlab.com/gitlab-org/gitlab-development-kit/blob/main/doc/howto/pages.md) through enabling `gitlab_pages['enable'] = true;`, setting up an external URL and the adding of the GitLab Pages hostname and GitLab Instance IP into the `etc/hostfile` on the container that runs the specs, so it can access the page on the GitLab container.

To run tests against the GitLab container, a GitLab QA (`gitlab/gitlab-qa`)
container is spun up and tests are run from it by running the
`Test::Instance::All` scenario with the `:gitlab_pages` tag.

Example:

```shell
$ gitlab-qa Test::Integration::GitlabPages EE
```

### `Test::Integration::LDAPNoTLS CE|EE|<full image address>`

This tests that a GitLab instance works as expected with an external
LDAP server with TLS not enabled.

The scenario spins up an OpenLDAP server, seeds users, and verifies
that LDAP-related features work as expected.

To run tests against the GitLab containers, a GitLab QA (`gitlab/gitlab-qa`)
container is spun up and tests are run from it by running the
`Test::Integration::LDAPNoTLS` scenario (located under
[`gitlab-org/gitlab/qa/qa/scenario/test/integration/ldap_no_tls.rb`][test-integration-ldap-no-tls]
in the GitLab project).

In EE, both the GitLab standard and LDAP credentials are needed:

1. The first is used to login as an Admin to enter in the EE license.
2. The second is used to conduct LDAP-related tasks

**Required environment variables:**

- [For EE only] `EE_LICENSE`: A valid EE license.

Example:

```shell
$ gitlab-qa Test::Integration::LDAPNoTLS CE

# For EE
$ export EE_LICENSE=$(cat /path/to/Geo.gitlab_license)

$ gitlab-qa Test::Integration::LDAPNoTLS EE
```

[test-integration-ldap-no-tls]: https://gitlab.com/gitlab-org/gitlab/blob/master/qa/qa/scenario/test/integration/ldap_no_tls.rb

### `Test::Integration::LDAPTLS CE|EE|<full image address>`

This tests that a TLS enabled GitLab instance works as expected with an external TLS enabled LDAP server.
The self-signed TLS certificate used for the GitLab instance and the private key is located at: [`gitlab-org/gitlab-qa@tls_certificates/gitlab`][test-integration-ldap-tls-certs]

The certificate was generated with OpenSSL using this command:

```shell
openssl req -x509 -newkey rsa:4096 -keyout gitlab.test.key -out gitlab.test.crt -days 3650 -nodes -subj "/C=US/ST=CA/L=San Francisco/O=GitLab/OU=Org/CN=gitlab.test"
```

The scenario spins up a TLS enabled OpenLDAP server, seeds users, and verifies
that LDAP-related features work as expected.

To run tests against the GitLab containers, a GitLab QA (`gitlab/gitlab-qa`)
container is spun up and tests are run from it by running the
`Test::Integration::LDAPTLS` scenario (located under
[`gitlab-org/gitlab/qa/qa/scenario/test/integration/ldap_tls.rb`][test-integration-ldap-tls]
in the GitLab project).

In EE, both the GitLab standard and LDAP credentials are needed:

1. The first is used to login as an Admin to enter in the EE license.
2. The second is used to conduct LDAP-related tasks

**Required environment variables:**

- [For EE only] `EE_LICENSE`: A valid EE license.

Example:

```shell
$ gitlab-qa Test::Integration::LDAPTLS CE

# For EE
$ export EE_LICENSE=$(cat /path/to/Geo.gitlab_license)

$ gitlab-qa Test::Integration::LDAPTLS EE
```

[test-integration-ldap-tls]: https://gitlab.com/gitlab-org/gitlab/blob/master/qa/qa/scenario/test/integration/ldap_tls.rb
[test-integration-ldap-tls-certs]: https://gitlab.com/gitlab-org/gitlab/blob/master/tls_certificates/gitlab

### `Test::Integration::LDAPNoServer EE|<full image address>`

This configures a GitLab instance for use with LDAP but does not
spin up an LDAP server in a docker container.

The LDAP server is created at runtime by the spec so that
the test can provide the fixture data for the LDAP server
as needed.

To run tests against the GitLab containers, a GitLab QA (`gitlab/gitlab-qa`)
container is spun up and tests are run from it by running the
`Test::Integration::LDAPNoServer` scenario (located under
[`gitlab-org/gitlab@qa/qa/scenario/test/integration/ldap_no_server.rb`](https://gitlab.com/gitlab-org/gitlab/blob/master/qa/qa/scenario/test/integration/ldap_no_server.rb)
in the GitLab project).

In GitLab project, both the GitLab standard and LDAP credentials are needed:

1. The first is used to login as an Admin to enter in the GitLab license.
2. The second is used to conduct LDAP-related tasks

**Required environment variables:**

- `EE_LICENSE`: A valid Enterprise license.

Example:

```shell
$ export EE_LICENSE=$(cat /path/to/GitLab.gitlab_license)

$ gitlab-qa Test::Integration::LDAPNoServer EE
```

### `Test::Integration::GroupSAML EE|<full image address>`

This tests that Group SAML login works as expected with an external SAML identity provider (idp).

This scenario spins up a SAML idp provider and verifies that a user is able to login to a group
in GitLab that has SAML SSO enabled.

To run tests against the GitLab containers, a GitLab QA (`gitlab/gitlab-qa`)
container is spun up and tests are run from it by running the
`Test::Integration::GroupSAML` scenario (located under [`gitlab-org/gitlab-ce@qa/qa/ee/scenario/test/integration/group_saml.rb`][test-integration-group-saml] in the GitLab EE project).

[test-integration-group-saml]: https://gitlab.com/gitlab-org/gitlab-ee/blob/master/qa/qa/ee/scenario/test/integration/group_saml.rb

**Required environment variables:**

- `EE_LICENSE`: A valid EE license.

Example:

```shell
$ export EE_LICENSE=$(cat /path/to/Geo.gitlab_license)

$ gitlab-qa Test::Integration::GroupSAML EE
```

### `Test::Integration::InstanceSAML CE|EE|<full image address>`

This tests that a GitLab instance works as expected with an external
SAML identity provider (idp).

This scenario spins up a SAML idp provider and verifies that a user is able to login to GitLab instance
using SAML.

To run tests against the GitLab containers, a GitLab QA (`gitlab/gitlab-qa`)
container is spun up and tests are run from it by running the
`Test::Integration::InstanceSAML` scenario (located under [`gitlab-org/gitlab-ce@qa/qa/scenario/test/integration/instance_saml.rb`][test-integration-instance-saml] in the GitLab project).

[test-integration-instance-saml]: https://gitlab.com/gitlab-org/gitlab/blob/master/qa/qa/scenario/test/integration/instance_saml.rb

**Required environment variables:**

- [For EE only] `EE_LICENSE`: A valid EE license.

Example:

```shell
$ gitlab-qa Test::Integration::InstanceSAML CE

# For EE
$ export EE_LICENSE=$(cat /path/to/Geo.gitlab_license)

$ gitlab-qa Test::Integration::InstanceSAML EE
```

### `Test::Instance::Image CE|EE|<full image address> --omnibus-config github_oauth`

This tests that users can sign in to a GitLab instance using external OAuth services.

The tests currently integrate with the following OAuth service providers:
* GitHub

To run tests against the GitLab containers, a GitLab QA (`gitlab/gitlab-qa`)
container is spun up and tests are run from it by running the `Test::Instance::All` scenario

**Required environment variables:**

- [For EE only] `EE_LICENSE`: A valid EE license.
- `QA_GITHUB_OAUTH_APP_ID`: Client ID for GitHub OAuth app. This can be found in the shared 1Password vault.
- `QA_GITHUB_OAUTH_APP_SECRET`: Client Secret for GitHub OAuth app. This can be found in the shared 1Password vault.
- `QA_GITHUB_USERNAME`: Username for authenticating with GitHub. This can be found in the shared 1Password vault.
- `QA_GITHUB_PASSWORD`: Password for authenticating with GitHub. This can be found in the shared 1Password vault.
- `QA_1P_EMAIL`: Email address for authenticating into gitlab-qa-totp 1Password vault. This can be found in the Gitlab-QA 1Password vault.
- `QA_1P_PASSWORD`: Password for authenticating into gitlab-qa-totp 1Password vault. This can be found in the Gitlab-QA 1Password vault.
- `QA_1P_SECRET`: Secret for authenticating into gitlab-qa-totp 1Password vault. This can be found in the Gitlab-QA 1Password vault.
- `QA_1P_GITHUB_UUID`: UUID for gitlab-qa-github item in gitlab-qa-totp 1Password vault. This can be found in the Gitlab-QA 1Password vault. The gitlab-qa-totp vault can be accessed with the the creds provided in 1P under "GitLab-QA 1Password user with access to gitlab-qa-totp vault"
- `QA_GITLAB_HOSTNAME`: Hostname set for GitLab instance. This must be set to `gitlab` as the external OAuth applications have been setup to use this.
- `QA_GITLAB_USE_TLS`: Use TLS for setting up GitLab instance. This must be set to `true` as since some external OAuth applications require the client to use TLS.

Example:

```
$ export QA_GITHUB_OAUTH_APP_ID=your_github_oauth_client_id
$ export QA_GITHUB_OAUTH_APP_SECRET=your_github_oauth_client_secret
$ export QA_GITHUB_USERNAME=your_github_username
$ export QA_GITHUB_PASSWORD=your_github_password
$ export QA_1P_EMAIL=1password_email
$ export QA_1P_PASSWORD=1password_password
$ export QA_1P_SECRET=1password_secret
$ export QA_1P_GITHUB_UUID=1password_gitlab-qa-github_item_uuid

$ gitlab-qa Test::Integration::OAuth CE

# For EE
$ export EE_LICENSE=$(cat /path/to/gitlab_license)

$ gitlab-qa Test::Integration::OAuth EE
```

### `Test::Integration::Packages CE|EE|<full image address>`

**Note: This Scenario no longer exists.  See https://gitlab.com/gitlab-org/gitlab-qa/-/merge_requests/662**

To run Packages tests, you may [configure Omnibus](configuring_omnibus.md) to use the [Packages](https://gitlab.com/gitlab-org/gitlab-qa/-/blob/master/lib/gitlab/qa/runtime/omnibus_configurations/packages.rb) configurator.

Example:

```shell
$ export EE_LICENSE=$(cat /path/to/Geo.gitlab_license)

$ gitlab-qa Test::Instance::Image EE --omnibus-config packages
```

This tests the GitLab Package Registry feature by setting
`gitlab_rails['packages_enabled'] = true` in the Omnibus configuration
before starting the GitLab container.

To run tests against the GitLab container, a GitLab QA (`gitlab/gitlab-qa`)
container is spun up and tests are run from it by running the
`Test::Instance::All` scenario with the `--tag packages` RSpec parameter,
which runs only the tests with `:packages` metadata.

**Required environment variables:**

- `EE_LICENSE`: A valid EE license.

### `Test::Integration::Praefect CE|EE|<full image address>`

This tests [Praefect](https://docs.gitlab.com/ee/administration/gitaly/praefect.html),
which is a reverse-proxy for Gitaly. It sets the Omnibus configuration
to use Praefect as the default storage backed by a single Gitaly node
before starting the GitLab container.

To run tests against the GitLab container, a GitLab QA (`gitlab/gitlab-qa`)
container is spun up and tests are run from it by running the
`Test::Instance::All` scenario.

Example:

```shell
$ gitlab-qa Test::Integration::Praefect EE
```

### `Test::Integration::SMTP CE|EE|<full image address>`

This tests SMTP notification email delivery from Gitlab by using
[MailHog](https://github.com/mailhog/MailHog) as MTA.
It starts up a Docker container for MailHog and sets the Omnibus configuration
to use it for SMTP delivery. The MailHog container will expose the configured
port for SMTP delivery, and also another port for the HTTP MailHog API used for
querying the delivered messages.

To run tests against the GitLab container, a GitLab QA (`gitlab/gitlab-qa`)
container is spun up and tests are run from it by running the
`Test::Integration::SMTP` scenario.

Example:

```shell
$ gitlab-qa Test::Integration::SMTP CE
```

### `Test::Integration::Jira CE|EE|<full image address>`

This tests that a GitLab instance works as expected with an external
Jira server.
It starts up a Docker container for Jira Server and another container
for GitLab.

To run tests against the GitLab container, a GitLab QA (`gitlab/gitlab-qa`)
container is spun up and tests are run from it by running the
`Test::Integration::Jira` scenario.

**Required environment variables:**

- [For EE only] `EE_LICENSE`: A valid EE license.
- `JIRA_ADMIN_USERNAME`: Username for authenticating with Jira server as admin.
- `JIRA_ADMIN_PASSWORD`: Password for authenticating with Jira server as admin.

These values can be found in the shared GitLab QA 1Password vault.

Example:

```shell
$ export JIRA_ADMIN_USERNAME=<jira_admin_username>
$ export JIRA_ADMIN_PASSWORD=<jira_admin_password>

# For EE
$ export EE_LICENSE=$(cat /path/to/GitLab.gitlab_license)

$ gitlab-qa Test::Integration::Jira EE
```

### `Test::Integration::Integrations CE|EE|<full image address>`

This scenario is intended to test the different integrations that a GitLab instance can offer, such as WebHooks to an external service, Jenkins, etc.

To run tests against the GitLab container, a GitLab QA (`gitlab/gitlab-qa`)
container is spun up and tests are run from it by running the
`Test::Integration::Integrations` scenario.

Example:

```shell
$ gitlab-qa Test::Integration::Integrations EE
```

### `Test::Instance::Any CE|EE|<full image address>:nightly|latest|any_tag http://your.instance.gitlab`

This tests that a live GitLab instance works as expected by running tests
against it.

To run tests against the GitLab instance, a GitLab QA (`gitlab/gitlab-qa`)
container is spun up and tests are run from it by running the
`Test::Instance` scenario (located under
[`gitlab-org/gitlab/qa/qa/scenario/test/instance.rb`][test-instance] in the
GitLab project).

Example:

```shell
$ export GITLAB_USERNAME=your_username
$ export GITLAB_PASSWORD=your_password

# Runs the QA suite for an instance running GitLab CE 10.8.1
$ gitlab-qa Test::Instance::Any CE:10.8.1-ce https://your.instance.gitlab

# Runs the QA suite for an instance running GitLab EE 10.7.3
$ gitlab-qa Test::Instance::Any EE:10.7.3-ee https://your.instance.gitlab

# You can even pass a gitlab-{ce,ee}-qa image directly
$ gitlab-qa Test::Instance::Any registry.gitlab.com:5000/gitlab/gitlab-ce-qa:v11.1.0-rc12 https://your.instance.gitlab
```

### `Test::Instance::Staging`

This scenario tests that the [`staging.gitlab.com`](https://staging.gitlab.com)
works as expected by running tests against it.

To run tests against the GitLab instance, a GitLab QA (`gitlab/gitlab-qa`)
container is spun up and tests are run from it by running the
`Test::Instance` scenario (located under
[`gitlab-org/gitlab/qa/qa/scenario/test/instance.rb`][test-instance] in the
GitLab project).

**Required environment variables:**

- `GITLAB_QA_USER_AGENT`: The browser user-agent to use instead of the default Chrome user-agent.
  This is needed for the automated tests to bypass the WAF

- `GITLAB_QA_ACCESS_TOKEN`: A valid personal access token with the `api` scope.
  This is used to retrieve the version that staging is currently running.
  This can be found in the shared 1Password vault.

- `GITLAB_USERNAME`: An existing user.

- `GITLAB_PASSWORD`: The user's password.

**Required by specific tests:**

- `QA_PRAEFECT_REPOSITORY_STORAGE`: The name of a Gitaly Cluster storage.

- `GITLAB_ADMIN_USERNAME`: An existing user with administrator access. Required by tests that set feature flags or
  perform other admin actions.

- `GITLAB_ADMIN_PASSWORD`: The administrator user's password.

**Optional environment variables:**

- `GITLAB_QA_DEV_ACCESS_TOKEN`: A valid personal access token for the
  `gitlab-qa-bot` on `dev.gitlab.org` with the `registry` scope.
  This is used to pull the QA Docker image from the Omnibus GitLab `dev` Container Registry.
  If the variable isn't present, the QA image from Docker Hub will be used.
  This can be found in the shared 1Password vault.
  Please note that this variable must be provided when you need to be sure the version of the
  tests matches the version of GitLab on Staging. If the version from Docker Hub is used it might not include changes deployed to Staging very recently.

An example of how to run the smoke tests:

```shell
$ export GITLAB_QA_USER_AGENT="<value from 1Password>"
$ export GITLAB_QA_ACCESS_TOKEN="<value from 1Password>"
$ export GITLAB_QA_DEV_ACCESS_TOKEN="<value from 1Password>"
$ export GITLAB_USERNAME="gitlab-qa"
$ export GITLAB_PASSWORD="<value from 1Password>"
$ export GITLAB_ADMIN_USERNAME="<value from 1Password>"
$ export GITLAB_ADMIN_PASSWORD="<value from 1Password>"
$ export GITLAB_QA_USERNAME_1="gitlab-qa-user1"
$ export GITLAB_QA_PASSWORD_1="<value from 1Password>"
$ export QA_PRAEFECT_REPOSITORY_STORAGE="nfs-file22"

$ gitlab-qa Test::Instance::Staging -- --tag smoke
```

#### Testing Against Different Cells

When running tests against GitLab instances that use a [cells architecture](https://docs.gitlab.com/ee/architecture/blueprints/cells/), you can target specific cells by setting the GitLab session cookie. This is useful for testing cell-specific functionality or debugging issues within a particular cell.

To direct tests to a specific cell, use the `QA_COOKIES` environment variable to set the `_gitlab_session` cookie with the target cell identifier:

```shell
$ export QA_COOKIES="_gitlab_session=cell-2-please"
$ gitlab-qa Test::Instance::Staging
```

**Note:** The actual cell identifiers depend on your GitLab instance configuration. Check with your GitLab administrator for the correct cell names in your environment.

### `Test::Instance::StagingRef`

This scenario tests that the [`Staging Ref`](https://staging-ref.gitlab.com)
works as expected by running tests against it.

To run tests against the GitLab instance, a GitLab QA (`gitlab/gitlab-qa`)
container is spun up and tests are run from it by running the
`Test::Instance` scenario (located under
[`gitlab-org/gitlab/qa/qa/scenario/test/instance.rb`][test-instance] in the
GitLab project).

**Required environment variables:**

- `GITLAB_QA_ACCESS_TOKEN`: A valid personal access token with the `api` scope.
  This is used to retrieve the version that staging is currently running.
  Staging Ref QA users credentials can be found in the shared GitLab QA 1Password vault.

**Optional environment variables:**

- `GITLAB_QA_DEV_ACCESS_TOKEN`: A valid personal access token for the
  `gitlab-qa-bot` on `dev.gitlab.org` with the `registry` scope.
  This is used to pull the QA Docker image from the Omnibus GitLab `dev` Container Registry.
  If the variable isn't present, the QA image from Docker Hub will be used.
  This can be found in the shared GitLab QA 1Password vault.

Example:

```shell
$ export GITLAB_QA_ACCESS_TOKEN=your_api_access_token
$ export GITLAB_QA_DEV_ACCESS_TOKEN=your_dev_registry_access_token
$ export GITLAB_USERNAME="gitlab-qa"
$ export GITLAB_PASSWORD="$GITLAB_QA_PASSWORD"

$ gitlab-qa Test::Instance::StagingRef
```

### `Test::Instance::StagingRefGeo`

This scenario tests that the Geo staging deployment (with [`staging-ref.gitlab.com`](https://staging-ref.gitlab.com) as the primary site and [`geo.staging-ref.gitlab.com`](https://geo.staging-ref.gitlab.com) as the secondary site) works as expected by running tests tagged `:geo` against it. This is done by spinning up a GitLab QA (`gitlab/gitlab-qa`) container and running the `QA::EE::Scenario::Test::Geo` scenario. Note that the Geo setup steps in the `QA::EE::Scenario::Test::Geo` scenario are skipped when testing a live Geo deployment.

**Required user properties:**

- The user must provide OAuth authorization on the secondary site before running Geo tests. This can be done via the authorization modal that appears after logging into the secondary node for the first time.

- Some Geo tests require the user to have Admin access level (for example, the Geo Nodes API tests)

**Required environment variables:**

- `GITLAB_QA_ACCESS_TOKEN`: A valid personal access token with the `api` scope.
  This is used to retrieve the version that staging is currently running.
  This can be found in the shared 1Password vault.

**Optional environment variables:**

- `GITLAB_QA_DEV_ACCESS_TOKEN`: A valid personal access token for the
  `gitlab-qa-bot` on `dev.gitlab.org` with the `registry` scope.
  This is used to pull the QA Docker image from the Omnibus GitLab `dev` Container Registry.
  If the variable isn't present, the QA image from Docker Hub will be used.
  This can be found in the shared 1Password vault.

```shell
$ export GITLAB_QA_ACCESS_TOKEN=your_api_access_token
$ export GITLAB_QA_DEV_ACCESS_TOKEN=your_dev_registry_access_token
$ export GITLAB_USERNAME="gitlab-qa"
$ export GITLAB_PASSWORD="$GITLAB_QA_PASSWORD"

$ gitlab-qa Test::Instance::StagingRefGeo
```

### `Test::Instance::Production`

This scenario functions the same as `Test::Instance::Staging`
but will run tests against [`gitlab.com`](https://gitlab.com).

In release 11.6 it is possible to test against the canary stage of production
by setting `QA_COOKIES=gitlab_canary=true`. This adds a cookie
to all web requests which will result in them being routed
to the canary fleet.

**Required environment variables:**

- `GITLAB_QA_USER_AGENT`: The browser user-agent to use instead of the default Chrome user-agent.
  This is needed for the automated tests to bypass the WAF

- `GITLAB_QA_ACCESS_TOKEN`: A valid personal access token with the `api` scope.
  This is used to retrieve the version that staging is currently running.
  This can be found in the shared 1Password vault.

- `GITLAB_USERNAME`: An existing user.

- `GITLAB_PASSWORD`: The user's password.

**Required by specific tests:**

- `GITLAB_QA_USERNAME_1`: The username of a pre-generated test user.

- `GITLAB_QA_PASSWORD_1`: The pre-generated test user's password.

**Optional environment variables:**

- `GITLAB_QA_DEV_ACCESS_TOKEN`: A valid personal access token for the
  `gitlab-qa-bot` on `dev.gitlab.org` with the `registry` scope.
  This is used to pull the QA Docker image from the Omnibus GitLab `dev` Container Registry.
  If the variable isn't present, the QA image from Docker Hub will be used.
  This can be found in the shared 1Password vault.
  Please note that this variable should be provided when you need to be sure the version of the
  tests matches the version of GitLab on Staging. If the version from Docker Hub is used it might not include changes deployed to Staging very recently.

An example of how to run the smoke tests:

```shell
$ export GITLAB_QA_USER_AGENT="<value from 1Password>"
$ export GITLAB_QA_ACCESS_TOKEN="<value from 1Password>"
$ export GITLAB_QA_DEV_ACCESS_TOKEN="<value from 1Password>"
$ export GITLAB_USERNAME="gitlab-qa"
$ export GITLAB_PASSWORD="<value from 1Password>"
$ export GITLAB_QA_USERNAME_1="gitlab-qa-user1"
$ export GITLAB_QA_PASSWORD_1="<value from 1Password>"
$ export QA_CAN_TEST_GIT_PROTOCOL_V2="false"
$ export QA_CAN_TEST_ADMIN_FEATURES="false"
$ export QA_CAN_TEST_PRAEFECT="false"

$ gitlab-qa Test::Instance::Production -- --tag smoke
```

### `Test::Instance::Preprod`

This scenario functions the same as `Test::Instance::Staging`
but will run tests against [`pre.gitlab.com`](https://pre.gitlab.com).

Note that [`pre.gitlab.com`](https://pre.gitlab.com) is used as an Interim
Performance Testbed and [will be replaced with the actual testbed in the future](https://gitlab.com/groups/gitlab-com/gl-infra/-/epics/60).

### `Test::Instance::Release`

This scenario functions the same as `Test::Instance::Staging`
but will run tests against [`release.gitlab.net`](https://release.gitlab.net).

### `Test::Instance::Smoke`

This scenario will run a limited amount of tests selected from the test suite tagged by `:smoke`.
Smoke tests are quick tests that ensure that some basic functionality of GitLab works.

To run tests against the GitLab instance, a GitLab QA (`gitlab/gitlab-qa`)
container is spun up and tests are run from it by running the
`Test::Instance::Smoke` scenario (located under
[`gitlab-org/gitlab/qa/qa/scenario/test/smoke.rb`][smoke-instance] in the
GitLab project).

Example:

```shell
$ gitlab-qa Test::Instance::Smoke ee:<tag> https://staging.gitlab.com
```

### `Test::Instance::RepositoryStorage`

This scenario will run a limited number of tests that are tagged with `:repository_storage`.

These tests verify features related to multiple repository storages.

**Required environment variables:**

- `QA_ADDITIONAL_REPOSITORY_STORAGE`: The name of the non-default repository storage.
- `QA_PRAEFECT_REPOSITORY_STORAGE`: The name of the gitaly cluster (praefect) repository storage.
- `QA_GITALY_NON_CLUSTER_STORAGE`: The name of a non gitaly cluster repository storage.

Example:

```shell
$ export QA_ADDITIONAL_REPOSITORY_STORAGE=secondary
$ export QA_GITALY_NON_CLUSTER_STORAGE=gitaly
$ export QA_PRAEFECT_REPOSITORY_STORAGE=praefect

$ gitlab-qa Test::Instance::RepositoryStorage
```

### `Test::Instance::Airgapped`

This scenario will run tests from the test suite against an airgapped instance.
The airgapped instance is set up by using `iptables` in the GitLab container to block network traffic other than testable ports, and by using runners
in a shared internal network.

Example:

```shell
# For EE
$ export EE_LICENSE=$(cat /path/to/gitlab_license)

$ gitlab-qa Test::Instance::Airgapped EE -- --tag smoke
```

### `Test::Instance::Image CE|EE|<full image address> --omnibus-config object_storage`

This orchestrated scenario runs tests from the test suite against a GitLab instance which has object storage configured instead of using local storage. The omnibus configuration `object_storage` ([configurator](configuring_omnibus.md)), uses Minio and `object_storage_aws` uses an AWS S3 account with credentials configured in the pipeline as CI Variables. To use Google Cloud Storage pass `--omnibus-config object_storage_gcp`. According to the storage selected it requires:

| Scenario              | Variable            | Description                          |
|-----------------------|---------------------|--------------------------------------|
| AWS S3 Object Storage | AWS_S3_REGION       | AWS region where bucket is created   |
| AWS S3 Object Storage | AWS_S3_KEY_ID       | AWS credentials                      |
| AWS S3 Object Storage | AWS_S3_ACCESS_KEY   | AWS credentials                      |
| AWS S3 Object Storage | AWS_S3_BUCKET_NAME  | Name of the bucket set in AWS        |
| GCP Object Storage    | GCS_BUCKET_NAME     | Name of the bucket set in AWS        |
| GCP Object Storage    | GOOGLE_JSON_KEY     | JSON key credential                  |
| GCP Object Storage    | GOOGLE_CLIENT_EMAIL | Email address of the service account |
| GCP Object Storage    | GOOGLE_PROJECT      | GCP project name                     |

These variables are available at 1Password QA Vault.

----

#### `Test::Instance::SecureVersion EE|<full image address>`

Used for testing that a project with Secure scanner functionality functions correctly against the specified version.
As this is Secure functionality, an EE license is required.
Can be used for running older instances of GitLab, this skips the Security version modal on login.

The script applies the license, imports the project, kicks off a pipeline, and checks that the pipeline was successful.

**Required environment variables:**

- `EE_LICENSE`: A valid EE license for testing.
- `PROJECT_URL`: The URL of the project to test.
- `PROJECT_NAME`: The name of the project to test, when imported to the instance.

Example:

```shell
$ export EE_LICENSE=$(cat /path/to/gitlab_license)
$ PROJECT_URL='https://gitlab.com/gitlab-org/quality/ci/secure-testing/secure-matrix-test.git' PROJECT_NAME='secure-matrix-test' gitlab-qa Test::Instance::SecureVersion gitlab/gitlab-ee:16.5.0-ee.0
```

### `Test::Integration::RegistryTLS EE`

It uses GitLab's TLS certificate found in the [`tls_certificates`](https://gitlab.com/gitlab-org/gitlab-qa/-/blob/master/tls_certificates/gitlab/gitlab.test.crt) folder.

To run a scenario with an insecure registry enabled use `Test::Integration::Registry EE` instead (it does not require the above certificate).

### `Test::Integration::RegistryTLS EE --omnibus-config object_storage`

This scenario is a composition of two orchestrated scenarios. It tests the container registry (TLS-enabled) integrated with an object storage backend.

An example would be to pass the option `--omnibus-config object_storage_aws` and the registry will be pulling and pushing images using AWS Cloud Storage as a storage backend.

```shell
gitlab-qa Test::Integration::RegistryTLS EE --omnibus-config object_storage_aws
```

### `Test::Instance::Image EE --omnibus-config decomposition_single_db`

**Note: The default Omnibus config is using a single database
with [two database connections](https://docs.gitlab.com/omnibus/settings/database.html#configuring-multiple-database-connections)**

This scenario is to run tests against a GitLab instance using a single database with only one `main` connection:

```ruby
gitlab-qa Test::Instance::Image EE --omnibus-config decomposition_single_db
```

### `Test::Instance::Image EE --omnibus-config decomposition_multiple_db`

This scenario is to run tests against a GitLab instance using [multiple databases](https://docs.gitlab.com/ee/administration/postgresql/multiple_databases.html):

```ruby
gitlab-qa Test::Instance::Image EE --omnibus-config decomposition_multiple_db
```

### `Test::Instance::Geo EE|<full image address>:nightly|latest|any_tag http://geo-primary.gitlab http://geo-secondary.gitlab`

This scenario will run tests tagged `:geo` against a live Geo deployment, by spinning up a GitLab QA (`gitlab/gitlab-qa`)
container and running the `QA::EE::Scenario::Test::Geo` scenario. Note that the Geo setup steps in the `QA::EE::Scenario::Test::Geo` scenario are skipped when testing a live Geo deployment. The URLs for the primary site and secondary site of the live Geo deployment must be provided as command line arguments.

**Required user properties:**

- The user must provide OAuth authorization on the secondary site before running Geo tests. This can be done via the authorization modal that appears after signing in to the secondary node for the first time.

- Some Geo tests require the user to have Admin access level (for example, the Geo Nodes API tests)

Example:

```shell
$ export GITLAB_USERNAME="gitlab-qa"
$ export GITLAB_PASSWORD="$GITLAB_QA_PASSWORD"

$ gitlab-qa Test::Instance::Geo EE https://primary.gitlab.com https://secondary.gitlab.com
```

### `Test::Instance::Chaos`

This scenario will run a limited number of tests that are tagged with `:chaos`.

These tests are designed to verify that our systems can gracefully handle scenarios which may occur if
networking or connectivity type issues occur in various Gitlab components.
They make use of [toxiproxy](https://github.com/Shopify/toxiproxy) to act as a proxy allowing us to introduce
connectivity issues, via the [toxiproxy ruby client](https://github.com/Shopify/toxiproxy-ruby) in an E2E spec.

Sample Test:

```ruby
  context 'when a gitaly node is experiencing high latency' do
    it 'can create a project' do
      Toxiproxy[:gitaly1].toxic(:latency, latency: 30000).apply do
        expect(create_a_project).to be true
      end
    end
  end
```

Example:

```shell
$ gitlab-qa Test::Instance::Chaos
```

### `Test::Integration::Import`

This scenario will run specs tagged with `:import` tags.

These tests are designed to validate import functionality by importing projects from `GitHub` or another `GitLab` instance.
This scenario type spins up 2 gitlab instances and additionally an instance with simple http mock server.

Example:

```shell
$ gitlab-qa Test::Integration::Import
```

#### GitHub

Setup uses [smocker](https://smocker.dev/) mock server for mocking all interactions with `GitHub`. By default environment variable `QA_MOCK_GITHUB` is
set to `true` which adds host entry to `import-target` docker container to redirect all requests for `api.github.com` dns name to mock container.

All mock definitions are defined in [github.yml](https://gitlab.com/gitlab-org/gitlab/-/blob/master/qa/qa/fixtures/mocks/import/github.yml) file. When mock definitions
need to be updated, it can be useful to proxy all requests to real `GitHub` instance and recording all requests and responses.

#### GitLab

Testing import [by direct transfer](https://docs.gitlab.com/ee/user/group/import/#migrate-groups-by-direct-transfer-recommended) is done by spinning up 2 omnibus installations - `import-target` and `import-source`. In order for tests to work, application setting `allow_local_requests_from_web_hooks_and_services` must be enabled in target instance. This is automatically done by test process if environment variable `QA_ALLOW_LOCAL_REQUESTS` is set to `true`.

### `AiGateway` Scenarios

The following `AiGateway` scenarios spin up a GitLab Omnibus instance integrated with a test AI Gateway using [mocked models](https://gitlab.com/gitlab-org/modelops/applied-ml/code-suggestions/ai-assist#mocking-ai-model-responses). These scenarios help to verify various configurations of cloud licensing and seat assignment work as expected when authenticating for AI features on self-managed instances.

**Required environment variables:**

All scenarios below require the following environment variables, **_unless otherwise noted:_**

- `QA_EE_ACTIVATION_CODE`: An activation code for a Staging-generated Premium or Ultimate cloud license with a purchased Duo Pro or Duo Enterprise add-on. This can be found in the GitLab QA 1Password vault.
- `GITLAB_LICENSE_MODE`: Set to `test` in order to configure GitLab Omnibus with the correct license mode and `CUSTOMER_PORTAL_URL`.

Example:

```shell
$ export QA_EE_ACTIVATION_CODE="<value from 1Password>"
$ export GITLAB_LICENSE_MODE="test"
```

----

#### `Test::Integration::AiGateway EE|<full image address>`

- Runs tests tagged with `:ai_gateway`
- GitLab instance has a cloud license with either a Duo Pro add-on or Duo Enterprise add-on, and a seat is assigned to the admin user. Choose Duo Enterprise over Duo Pro when available.
- Serves the Duo Chat and Code Suggestions feature areas. It does not boot the Duo Workflow Service (DWS); the Duo Agent Platform foundational flow runs under the dedicated `Test::Integration::DuoAgentPlatform` scenario (below).

Example:

```shell
$ gitlab-qa Test::Integration::AiGateway EE
```

----

#### `Test::Integration::DuoAgentPlatform EE|<full image address>`

- Runs tests tagged with `:duo_agent_platform`
- Subclass of `Test::Integration::AiGateway`, so it inherits the same setup (cloud license + Duo add-on + assigned seat, plus the mock AI Gateway).
- In addition, this scenario boots a mock Duo Workflow Service (DWS) container in agentic-mock mode (`duo-workflow-service`, from the same `model-gateway` image switched to the DWS gRPC server) and points GitLab at it via the `GITLAB_DUO_WORKFLOW_SERVICE_URL` / `GITLAB_DUO_WORKFLOW_SECURE` environment variables. This lets the Duo Agent Platform foundational-flow spec drive a flow end-to-end without a real LLM.
- DWS container logs are captured to the job artifacts (`duo-workflow-service/logs/duo-workflow-service.log`) on teardown.

Example:

```shell
$ gitlab-qa Test::Integration::DuoAgentPlatform EE
```

----

#### `Test::Integration::AiGatewayNoSeatAssigned EE|<full image address>`

- Runs tests tagged with `:ai_gateway_no_seat_assigned`
- GitLab instance has a cloud license + Duo Pro or Duo Enterprise add-on, but no seat is assigned to the admin user.

Example:

```shell
$ gitlab-qa Test::Integration::AiGatewayNoSeatAssigned EE
```

----

#### `Test::Integration::AiGatewayNoAddOn EE|<full image address>`

- Runs tests tagged with `:ai_gateway_no_add_on`
- GitLab instance has a cloud license without a Duo Pro or Duo Enterprise add-on, and no seat is assigned to the admin user.

**Regarding environment variables:**

- `QA_EE_ACTIVATION_CODE` should be set to a Staging-generated Premium or Ultimate cloud license _without_ a Duo Pro or Duo Enterprise add-on.

Example:

```shell
$ gitlab-qa Test::Integration::AiGatewayNoAddOn EE
```

---

#### `Test::Integration::AiGatewayNoLicense EE|<full image address>`

- Runs tests tagged with `:ai_gateway_no_license`
- GitLab instance has no cloud license and no seat is assigned to the admin user

**Regarding environment variables:**

- `QA_EE_ACTIVATION_CODE` and `GITLAB_LICENSE_MODE` can be omitted

Example:

```shell
$ gitlab-qa Test::Integration::AiGatewayNoLicense EE
```

----

#### Troubleshooting `AiGateway` Tests

The following logs are available for troubleshooting `AiGateway` tests in the `e2e-test-on-omnibus` pipelines.

| Component            | Log Location                                                                                                                                                           | Additional Info                                                                                                                                       |
|----------------------|------------------------------------------------------------------------------------------------------------------------------------------------------------------------|-------------------------------------------------------------------------------------------------------------------------------------------------------|
| CustomersDot Staging | [Kibana](https://nonprod-log.gitlab.net/) (`pubsub-rails-inf-stgsub*`), [Sentry](https://new-sentry.gitlab.net/organizations/gitlab/issues/?environment=stg&project=8) | See [CustomersDot Logging](https://gitlab.com/gitlab-org/customers-gitlab-com/blob/main/doc/architecture/logging.md#customersdot-logging) for details |
| GitLab Omnibus       | Job artifacts: `gitlab/logs`                                                                                                                                           | See the GitLab [Log system](https://docs.gitlab.com/ee/administration/logs/) docs for more details                                                    |
| AI Gateway           | Job artifacts: `ai-gateway/logs/modelgateway_debug.log`                                                                                                                | See the AI Gateway's [README](https://gitlab.com/gitlab-org/modelops/applied-ml/code-suggestions/ai-assist#gitlab-ai-gateway) for more details        |

----

[Back to README.md](../README.md)
