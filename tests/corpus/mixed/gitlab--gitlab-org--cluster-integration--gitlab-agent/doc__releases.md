# Release Process

The `gitlab-agent` project supports [Managed Versioning](https://gitlab.com/gitlab-org/release/docs/-/tree/master/components/managed-versioning).
However, there are a few special things:

- Only KAS is released using [release-tools](https://gitlab.com/gitlab-org/release-tools).
- Agentk is released using the canonical [`gitlab-org/cluster-integration/gitlab-agent`](https://gitlab.com/gitlab-org/cluster-integration/gitlab-agent) project.
- GitLab Agent Helm Chart is released using the canonical [`gitlab-org/charts/gitlab-agent`](https://gitlab.com/gitlab-org/charts/gitlab-agent) project.

Managed Versioning was introduced in the context of https://gitlab.com/gitlab-com/gl-infra/delivery/-/issues/19960.

## Releasing a pre-release (alpha, beta and release candidates)

The pre-release process is automated by [Release Tools](https://handbook.gitlab.com/handbook/engineering/infrastructure/release-tools/).

The automated task runs every hour and checks if a new version is available in the Gitlab Agent project.
The task will create the MR if the new version is available.
The MR will be auto-merged if the pipeline is green.

The automated task runs as follows:
1. Check if the current version in GitLab Rails is the same as the latest successful git ref in the GitLab Agent project.
1. If the versions are the same, the task will end the process.
1. If the versions are different, the task will check if the merge request exists already.
1. If the merge request does not exist, the task will create a new merge request and set it to auto-merge.

For more details, see the [`UpdateKas`](https://gitlab.com/gitlab-org/release-tools/-/blob/master/lib/release_tools/tasks/components/update_kas.rb) task.

### Releasing a pre-release manually for updating Ruby gems

The `gitlab-agent` maintainers are responsible to release a pre-release manually when we need to update Ruby gems.

1. Bump the version in the [`/VERSION`](../VERSION) file to the version you want to release.
1. Create a Merge Request, and go through the review process and eventually merge the Merge Request with that version bump.
1. Create an **annotated** tag with the format `v<VERSION>` (where `VERSION` is what is in the `./VERSION` file)
   in the canonical [`gitlab-org/cluster-integration/gitlab-agent`](https://gitlab.com/gitlab-org/cluster-integration/gitlab-agent) project.
   (it's very important that the Git tag is annotated, because our build system relies on tag creation date
   for proper sorting.)
1. Review and merge the Merge Request created by the renovate bot in the canonical GitLab project.
   *(the Merge Request is automatically assigned to the [KAS Version Maintainers](https://gitlab.com/groups/gitlab-org/maintainers/kas-version-maintainers/-/group_members?with_inherited_permissions=exclude) and labeled with `~KAS`)*
1. [Release the GitLab Agent Helm Chart](#release-the-gitlab-agent-helm-chart).

### GitLab Agent release from a (MR) pipeline

The GitLab Agent can be "released" for testing purposes from any (MR) pipeline
by manually triggering the `push_image_manual` job.

## Releasing a monthly releases (for self-managed for each milestone)

The monthly release process is automated by release-tools.
The automated task runs as follows:
1. Create a stable branch.
1. Update the `VERSION` file to the new version and commit the change in the stable branch.
1. Compile the changelog.
1. Create a tag.
1. Update the `VERSION` file on the default branch to the next milestone's `-rc1` and tag it as `vX.Y.0-rc1`.
1. Notify Slack in `#announcements` channel.

For more details, see the [KasRelease](https://gitlab.com/gitlab-org/release-tools/-/blob/master/lib/release_tools/public_release/kas_release.rb) class.

We plan to automate the monthly release for GitLab Agent Helm Chart.
It is tracked in https://gitlab.com/gitlab-com/gl-infra/delivery/-/issues/20169.

## Releasing a patch release (includes all security releases)

The [GitLab release managers](https://about.gitlab.com/community/release-managers/)
are responsible for releasing a GitLab monthly and patch releases for all projects under
[managed versioning](https://gitlab.com/gitlab-org/release/docs/-/tree/master/components/managed-versioning), including KAS.

After the patch release has been published, security fixes will be mirrored to the canonical repository.
Note that during the patch release process (starting roughly two days before the actual release day)
the project mirroring may be broken in case security patches got merged into the stable branches
in the security mirror.

See the following documents for patching processes:

- [Backports](/doc/backports.md)
- [Security Patches](/doc/security_patch.md)

## Release the GitLab Agent Helm Chart

The GitLab Agent Helm Chart is not yet part of Managed Versioning.

The issue https://gitlab.com/gitlab-com/gl-infra/delivery/-/issues/20169 proposes to include the GitLab Agent Helm Chart into
Managed Versioning process.

1. Review and merge the Merge Request created by the renovate bot to update the `appVersion` in the canonical [GitLab Agent Helm Chart](https://gitlab.com/gitlab-org/charts/gitlab-agent) project.
1. Bump `version` according to semantic versioning. For a GitLab monthly release, this will generally be the minor version.
1. Push a tag `vX.Y.Z` where `X.Y.Z` is the `version` in `Chart.yaml`.

## Implementation Details

The following bullet point list highlights a few noteworthy implementation details about the `gitlab-agent` project release process.

- There are three GitLab projects that matter for the `gitlab-agent` release process:
    - The canonical project at https://gitlab.com/gitlab-org/cluster-integration/gitlab-agent.
    - The security mirror at https://gitlab.com/gitlab-org/security/cluster-integration/gitlab-agent.
    - The dev mirror at https://dev.gitlab.org/gitlab/cluster-integration/gitlab-agent.
- The canonical project has a push mirror to keep the security mirror in sync. All tags and protected branches are synced.
- The security mirror has a push mirror to keep the dev mirror in sync. All tags and protected branches are synced.
- Regular development happens in the canonical project.
- Undisclosed security development happens in the security mirror.
- The pipeline in the security mirror runs for tags as well as branches, and publishes the agentk and Relay
  images into the mirror's own, private registry. A security release is tagged there first; the canonical project
  receives the tag when the release is published, hours later. The Runway release jobs run in both projects for
  that reason; see [Releasing to Runway](theseus.md#releasing-to-runway).
- The pipeline in the dev mirror is not relevant.
- The SSoT of the binary versions being built by the `gitlab-agent` project `Makefile` and `bazel` builds is
  the contents of the `VERSION` file (no matter who builds it and where it is built).
- The KAS release schedule is aligned with GitLab releases, which means that
  the [monthly release](#releasing-a-monthly-releases-for-self-managed-for-each-milestone) is performed on the third Tuesday of the month
  and patch releases are performed on the second and fourth Wednesday of the month.
- The KAS release process is controlled by [Managed Versioning](https://gitlab.com/gitlab-org/release/docs/-/tree/master/components/managed-versioning)
  and [release-tools](https://gitlab.com/gitlab-org/release-tools).
- The [release-tools](https://gitlab.com/gitlab-org/release-tools) creates the `VERSION` file update, changelog commits
  and tags for [monthly](#releasing-a-pre-release-alpha-beta-and-release-candidates) and
  [patch](#releasing-a-patch-release) releases
  in the security mirror based on the `*-stable` branches.
- The [release-tools](https://gitlab.com/gitlab-org/release-tools) creates
  [monthly releases](#releasing-a-monthly-releases-for-self-managed-for-each-milestone)
  based off of the revision from the last Git tag in the *MAJOR.MINOR* lineage - that is most likely a release candidate.
- On the monthly release week preparation (done by release managers with help of [release-tools](https://gitlab.com/gitlab-org/release-tools)):
  - A stable branch is created based on a commit that is running on production.
  - A release candidate is tagged from the stable branch content. Additional bug fixes can be added on the GitLab release which might originate a new release candidate
  - A final tag is created from the last release candidate.
- New packages are tagged and published on patch releases based on the stable branch content.
- KAS Maintainers create the `VERSION` file update, the changelog commit and tag
  for [pre-releases](#releasing-a-pre-release-alpha-beta-and-release-candidates) in the canonical project.
- KAS is built by the CNG and Omnibus project for a GitLab release.
- The KAS images built by CNG are used by the GitLab Helm Chart (that includes the deployment on GitLab.com).
- The KAS binaries built by Omnibus are used for Omnibus in the context of GitLab self-managed installations.
- The version to link into the KAS binaries is taken from the `VERSION` file.
- agentk builds are not yet part of Managed Versioning.
- agentk is built in the canonical `gitlab-agent` project using pipelines that run for Git tags.
- The Relay image `relay` is published by the same tag pipelines; see
  [The Relay image](developing.md#the-relay-image).
- If Git refs are recreated in the canonical project (first and foremost branches and tags)
  they need to manually be removed in the security and then in the dev mirror after the deletion
  and before the creation of the recreate. Otherwise, there will be conflicts in the push mirror synced.
  In case this happens, KAS maintainers get an email notification and need to take action.

## References

- [Example renovate MR for the `GITLAB_KAS_VERSION` file](https://gitlab.com/gitlab-org/gitlab/-/merge_requests/111845)
- [List of bot-created MRs](https://gitlab.com/gitlab-org/gitlab/-/merge_requests?scope=all&state=all&label_name[]=group%3A%3Aenvironments&author_username=gitlab-dependency-update-bot)
- [Bot configuration](https://gitlab.com/gitlab-org/frontend/renovate-gitlab-bot/-/blob/main/renovate/gitlab/version-files.config.js)
- [Dependency dashboard](https://gitlab.com/gitlab-org/gitlab/-/issues/390663)

## Troubleshooting

### Image with desired tag is not on `dev.gitlab.org`

The image deployed to `dev.gitlab.org` is built within the pipeline of the
[security images project](https://gitlab.com/gitlab-org/security/charts/components/images) in a 3 hour frequency.
The version to built is taken from the [`GITLAB_KAS_VERSION`](https://gitlab.com/gitlab-org/gitlab/-/blob/master/GITLAB_KAS_VERSION).

### Image Pull Error in gstg / dev cluster

This may happen because the image from `dev.gitlab.org` is not yet synced with the high availability registry,
where the KAS image is eventually pulled from. This registry may lag 1.5-2 hours behind.
This sync happens in the `sync-images-artifact-registry` job of the pipeline in the
[security images project](https://gitlab.com/gitlab-org/security/charts/components/images).
