---
stage: GitLab Delivery
group: Operate
info: To determine the technical writer assigned to the Stage/Group associated with this page, see <https://handbook.gitlab.com/handbook/product/ux/technical-writing/#assignments>
title: Helm rendering with internal/render
---

The `internal/render` package templates Helm charts into Kubernetes objects. It makes no cluster
calls and keeps no release records. Use it for every new controller that renders a chart.

The deprecated `helm/` package still renders the `v1beta1` resources and is frozen.

| Resources | Renderer | Consumers |
|---|---|---|
| `v1beta1` | Deprecated `helm/` | GitLab controller and webhook in `controllers/` |
| v2 | `internal/render` | Controllers in `internal/controller/` |

## Render a chart

Construct a new `Request` for every render. Nothing is shared, so concurrent renders are safe.

```go
result, err := render.Render(render.Request{
    ChartPath:   chartPath,
    ReleaseName: name,
    Namespace:   namespace,
    Values:      values,
})
```

Set these request fields:

- `ChartPath`: a packaged chart (`.tgz`) or an unpacked chart directory. Use
  `render.LocateChart(dir, name, version)` to resolve `<name>-<version>.tgz` in a charts directory,
  typically `settings.HelmChartsDirectory`.
- `ReleaseName`: required.
- `Namespace`: defaults to `default`.
- `Values`: user values, merged over the chart defaults.
- `KubeVersion` and `APIVersions`: the `.Capabilities` render context. Read them with
  `capabilities.Discover`.

Read these result fields:

- `Objects`: the rendered manifests as `*unstructured.Unstructured`, so foreign custom resources are
  first-class. The order is deterministic: sorted by kind following the Helm install order, with
  template path and in-file order preserved within a kind.
- `Hooks`: the manifests that declare a `helm.sh/hook` annotation. They never appear in `Objects`.
- `CRDs`: the definitions from the `crds/` directories of the chart and its dependencies. Apply them
  before the objects.
- `Notes`: the rendered `NOTES.txt` of the top-level chart. Notes of dependency charts are excluded.
- `Warnings`: the documents that were dropped. Check or log them.

Every render is composed as revision 1 of an install. `Request` carries no upgrade flag, so express
upgrade behavior through hook events.

## Read the capabilities of the target cluster

Read the cluster facts with `internal/render/capabilities` and pass them into the request:

```go
discovered, err := capabilities.Discover(discoveryClient)
if err != nil {
    return err
}

result, err := render.Render(render.Request{
    ChartPath:   chartPath,
    ReleaseName: name,
    Namespace:   namespace,
    Values:      values,
    KubeVersion: discovered.KubeVersion,
    APIVersions: discovered.APIVersions,
})
```

Always pass discovered capabilities. Charts branch on `.Capabilities.APIVersions.Has`. Without
`policy/v1/PodDisruptionBudget` in the set, the GitLab chart renders `PodDisruptionBudget` as
`policy/v1beta1`, which no current cluster serves, so every apply of that object fails.

A non-empty `APIVersions` replaces the Helm default set instead of extending it. A hand-written set
must be complete. List every entry the chart probes, in both the group-version form (`policy/v1`)
and the group-version-kind form (`policy/v1/PodDisruptionBudget`), because templates use both. Find
the probes with:

```shell
grep -r 'APIVersions.Has' <chart>
```

### Apply the operator settings

The renderer reads no process state, so `GITLAB_OPERATOR_KUBERNETES_API_VERSIONS` and
`GITLAB_OPERATOR_KUBERNETES_VERSION` do not reach it. Append them to the discovered set in the
controller:

```go
apiVersions := discovered.APIVersions
if len(settings.DefaultKubeAPIVersions) > 0 {
    apiVersions = append(apiVersions, settings.DefaultKubeAPIVersions...)
}
```

> [!note]
> Append rather than assign. The frozen `helm/` renderer treats the same variable as additive.
> Passing the setting straight to `Request.APIVersions` discards what discovery found.

## Offline rendering caveats

Templating without a cluster limits what chart constructs can see. The deprecated `helm/` package
renders with a client-side dry run and shares every caveat below, so moving a chart between the two
engines does not change these behaviors.

### The lookup function returns nothing

The `lookup` template function always returns an empty map, without an error and without a warning.
A template that branches on an existing object takes the "not found" path:

- A value preserved with the lookup-or-generate idiom, such as
  `(lookup "v1" "Secret" ...) | dig "data" "key" (randBytes 32)`, is regenerated on every render.
  Applying that output rotates the secret on every reconcile.
- A resource guarded by "render only when absent" is always rendered. The Gateway API safe-upgrade
  policy in the GitLab chart works this way.

Read cluster state in the controller and pass it in through `Request.Values`. Do not add a cluster
connection to the renderer.

### Rendered objects are incomplete

- They carry no release ownership metadata. Helm stamps the `app.kubernetes.io/managed-by` label and
  the `meta.helm.sh/release-name` and `meta.helm.sh/release-namespace` annotations on apply, not on
  render.
- They carry `metadata.namespace` only when the template sets it. The GitLab chart sets it on every
  namespaced object, but the dependency charts do not, so stamp the namespace before you apply.
- Their capabilities are only what the request carries.

### Renderer limits

- DNS is disabled, so `getHostByName` returns an empty string.
- Rendering is not strict, so a missing value renders as `<no value>` instead of failing.
- Hooks, CRDs, and `NOTES.txt` are returned as data, never executed, applied, or printed.
- No release history exists, so there is no rollback, no resource adoption, and no post-renderer
  support.
- Values are validated against the `values.schema.json` files of the chart and its dependencies, so
  a schema violation fails the render.

## Apply rendered objects

Rendered objects suit server-side apply. Every object has a name, none carries a `status` block, and
integers decode as `int64`, so an object serializes identically on every render. The end-to-end test
in `internal/render/hookexec/e2e_test.go` applies a whole chart this way, through `client.Apply`
with a field owner.

Do two things first:

- Stamp `metadata.namespace` on namespaced objects. The renderer never sets it.
- Prune explicit `null` values. Chart templates emit them and the renderer preserves them, so under
  server-side apply each one becomes a field your manager owns and declares empty.

A full GitLab render carries about 20 nulls, concentrated in a few paths:

- `metadata.annotations` and `spec.template.metadata.annotations`
- `spec.strategy.rollingUpdate`
- `spec.template.spec.containers[].env[].value`
- `metadata.creationTimestamp`

The legacy `helm/` renderer dropped most of these in its typed decode, so a chart moved to this
package applies more of what the templates actually say. The same typed decode also emitted an empty
`status` block on about a third of its objects, which this package never does.

## Hooks

Manifests that declare a `helm.sh/hook` annotation land in `Result.Hooks`. Each hook carries:

- `Events`: the trimmed, lowercased hook events, for example `pre-upgrade`. The legacy
  `test-success` alias resolves to `test`. A manifest with an unknown event is dropped with a typed
  warning.
- `Weight`: the `helm.sh/hook-weight` annotation. A missing or unparseable value yields 0.
- `DeletePolicies`: the `helm.sh/hook-delete-policy` values. Interpreting them is the caller's
  concern.

Call `Result.HooksFor(event)` to get the hooks of one event in the Helm execution order: ascending
weight, then object name.

### Run the hooks

Run the hooks of every release. The GitLab chart generates every secret it needs from its
`pre-install` hooks, so a release whose hooks never ran has no credentials and cannot start.

The `internal/render/hookexec` package runs them through the Helm kube client:

```go
client, err := hookexec.NewClient(restClientGetter, namespace)
if err != nil {
    return err
}

runner := hookexec.New(client, hookexec.WithTimeout(10*time.Minute))

if err := runner.Run(ctx, result, "pre-install"); err != nil {
    return err
}
```

- Build one Runner per release. `NewClient` requires a namespace, and a hook whose manifest omits
  `metadata.namespace` inherits it. Do not leave the namespace to the client getter: in-cluster it
  falls back to the namespace of the operator, and such a hook is created beside the operator rather
  than beside the release.
- Set `WithTimeout` for the GitLab chart. The default is the Helm CLI default of five minutes, which
  the shared-secrets Job can exceed while pulling the toolbox image.
- Pass any `kube.Interface` to `New`, so a caller that already holds a client can reuse it and tests
  can pass a fake.

The runner follows the Helm algorithm, which charts are written against:

- A hook with no `helm.sh/hook-delete-policy` annotation is treated as `before-hook-creation`. The
  delete before create is what makes a rerun possible, because a completed Job is immutable.
- Each hook is created and watched to completion before the next one starts, so a weight `-5`
  ServiceAccount exists before the weight `0` Job that uses it.
- After the event succeeds, hooks carrying `hook-succeeded` are deleted in reverse order. When a hook
  fails, that hook is deleted per `hook-failed` and the hooks that already succeeded are deleted per
  `hook-succeeded`.
- `CustomResourceDefinition` hooks are never deleted, to avoid cascading garbage collection.

Unlike `render` and `objects`, this package talks to the API server.

## Warnings

A document that is not valid YAML, lacks `apiVersion` or `kind`, or carries an unusable hook
annotation becomes a `Result.Warnings` entry with a typed `WarningReason` instead of being silently
dropped. Check or log the warnings in every consumer.

## Select and mutate rendered objects

Use the `internal/render/objects` package to select and mutate rendered object slices. The render
package carries no query or mutation layer.

Selection helpers never fail and preserve order. The returned slices alias the input objects, so
deep-copy an object before mutating it when the original must stay untouched:

```go
gated, rest := objects.Partition(result.Objects, objects.And(
    objects.ByKind("Deployment"),
    objects.Or(objects.ByComponent("webservice"), objects.ByComponent("sidekiq")),
))
```

Mutation helpers cover the zero-downtime upgrade operations on rendered, not-yet-applied workloads,
and fail on unexpected kinds or shapes:

- `SetPaused`, `SetReplicas`, `UnsetReplicas`, and `GetReplicas` control and inspect Deployment
  rollout and scale.
- `SetPodTemplateAnnotation` sets checksum and restart-trigger annotations.
- `UpsertInitContainerEnv`, `RemoveInitContainerEnv`, and `UpsertEnvInAllContainers` manage
  environment variables, for example the schema-version bypass.

## Release labels

`Render` marks every object and hook with the release identity:

```plaintext
operator.gitlab.com/release-name        the release name
operator.gitlab.com/release-namespace   its namespace
```

Use the exported keys to find the output again:

```go
client.MatchingLabels{render.ReleaseNameLabel: name}
```

The chart labels cannot do this. Across a full GitLab render, `release` and
`app.kubernetes.io/instance` each appear on about half the objects, and a few objects carry no labels
at all. Owner references do not help either: a cluster-scoped object may not reference a namespaced
custom resource, so it outlives the namespace with no other handle on it.

The labels go into `metadata.labels` only. The renderer never touches `spec.selector`, which is
immutable after creation, or the pod template labels, which would roll every pod on each reconcile. A
release name or namespace that cannot be a label value fails the render.

Two things stay unmarked:

- `Result.CRDs`, because the Helm SDK installs a `crds/` directory once and never updates it, and
  chart definitions are commonly shared between releases.
- Objects that a hook creates at runtime, such as the generated secrets. They are namespaced, so
  deleting the namespace removes them.

## Run the tests

The package tests are standard Go tests with `testify`. The fixture-chart tests are hermetic. The
full-GitLab-chart tests read `HELM_CHARTS` and `CHART_VERSION`, and skip when the chart archive is
missing locally:

```shell
task retrieve-charts
HELM_CHARTS=$(pwd)/charts CHART_VERSION=$(head -n1 CHART_VERSIONS) go test ./internal/render/...
```

CI runs the tests against every version in `CHART_VERSIONS`, and fails there when the archive is
missing.

### Run the end-to-end tests

`internal/render/hookexec/e2e_test.go` installs the chart into a real cluster: render once, apply
`Result.CRDs`, run the `pre-install` hooks, and only then apply the workloads. Use it as the worked
example for consuming hooks.

The test applies the chart definitions, and the GitLabCore reconciler never does. For more
information, see [The GitLabCore reconciler](gitlabcore.md).

The test is gated behind the `e2e` build tag, so the unit tests never pick it up. It creates and
deletes a namespace, so point kubectl at a throwaway cluster:

```shell
task e2e-tests
```

The hook phase runs by default. The other phases write cluster-wide, so they are opt-in:

| Variable | Description |
|---|---|
| `E2E_APPLY_CRDS=1` | Installs the chart definitions, which are cluster-wide and may collide with definitions already present. |
| `E2E_APPLY_WORKLOADS=1` | Applies the workloads. They need real PostgreSQL, Redis, and object storage to become ready. |
| `E2E_KEEP_NAMESPACE=1` | Keeps the namespace and the cluster-scoped objects for inspection. |

The test records every cluster-scoped object it applies and deletes it during cleanup. After a run
with `E2E_KEEP_NAMESPACE=1`, delete those objects by hand. An admission webhook whose backing service
is gone rejects unrelated writes across the whole cluster.
