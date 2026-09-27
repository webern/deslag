# Mixed-message and learned-module epoch proposal: independent review

Date: 2026-09-08 UTC
Reviewer: Vera, OpenAI Codex using GPT-6 Astra
Operational status: research-grade source/design review
Lifecycle: active
Disposition: accepted within the proposed bounded design scope
Work item: 081M1Z63YMC087G0R003N5FH9X

## Reviewed identity and disposition

The reviewed [clarified proposal](https://github.com/Lucent-Financial-Group/Zeta/blob/9928404da6cf7ab1c5e2b251ae7afe73bbe2eda0/docs/research/2026-09-08-mixed-message-learned-module-epoch-proposal.md)
is pinned at `9928404da6cf7ab1c5e2b251ae7afe73bbe2eda0`, following original
`17be7ee94cba7610a7896d66930442affff8c4bb`. The proposal is 23,841 bytes,
SHA256 `4DB5A6223C0ABCFFC9D8918AFB8D64CF48CB51178BFC165C1DB934F748E5158F`.
Its source census is 7,159 bytes,
SHA256 `F23598B7EE2249F774D916704A7C98E8FCBE03737C5F3499BCC0DDA1918A7FD3`.
Both equal the inspected owner files and their immutable Git blobs.

No material mathematical, model-identity or existing-code reuse error was found.
Acceptance is for this proposed finite adapter and its exit criteria. It is not
implementation approval, an executable registration, global inference exactness,
a learned-DAG result or an instruction to open any data or old study stream.
The clarification correctly requires projected-belief/base quotient and keeps
undamped certification separate from reconstructed or damped application.

The [retained identity audit](mixed-message-epoch/2026-09-08/independent-review/manifest.json)
verifies all 24 census entries at source
`15c634e096674c757f3656c0b8ff9aaa0b5cf203`: 578,099 bytes, exact lengths,
SHA256 values and Git blobs. All 24 also matched the inspected root files.
This verifies file identities, not complete dependency or runtime closure.

## Independent equation check

For fixed feature outputs `mu_i = h_theta_i(x)`, the proposed finite fixture is
an explicit product-factor model. With prior Gaussian natural parameters
`(eta0,t0)`, Gamma shape/rate priors `(a_i,b_i)`, and base measures `dz` and
`d gamma_i`, its kernel is

```text
p(z,gamma | x) proportional to
  exp(eta0*z - t0*z*z/2) * exp(k*z - c*exp(z))
  * product_i [ gamma_i^(a_i-1) * exp(-b_i*gamma_i)
                * gamma_i^(1/2) * exp(-gamma_i*(z-mu_i)^2/2) ].
```

Here `t0,a_i,b_i,c > 0`. The Gaussian prior controls the negative-z tail even
when the unary potential alone is improper; the positive exponential penalty
controls the positive tail. Integrating each Gamma factor yields a finite
multiple of `[b_i+(z-mu_i)^2/2]^(-a_i-1/2)`. Thus the stated proper priors and
finite fixed feature outputs define a normalizable joint product. This does
not assert empirical independence of expert prediction errors.

Under `q(z) product_i q(gamma_i)` with `q(z)=Normal(m,v)`, the coordinate rule is

```text
R_i = (m-mu_i)^2 + v
q_new(gamma_i) = Gamma(a_i+1/2, b_i+R_i/2)
site_to_gamma_i = GammaKernel(LogPower=1/2, Rate=R_i/2).
```

After that block commits, let `g_i=E_q[gamma_i]`. The Gaussian base for the next
block has `t=t0+sum_i g_i`, `eta=eta0+sum_i g_i*mu_i`, and `u=eta/t`.
The Normal message to z is `(PrecisionMean=g_i*mu_i, Precision=g_i)`.
These match `tryNormalPrecisionVmp` with the feature's variance set to zero;
clamping is explicit. Substituting BP cavities for the other-variable VMP
marginals changes this coordinate problem. The shape increment comes from the
Normal factor's `gamma_i^(1/2)` term and must not be omitted.

The unary Gaussian-family objective is the already admitted scalar objective

```text
F(m,v) = t*((m-u)^2+v)/2 - k*m + c*exp(m+v/2) - log(v)/2.
```

The proposal properly describes minimization of `KL(q || target)`, rather than
EP moment matching. The [VMP paper](https://www.jmlr.org/papers/v6/winn05a.html)
uses a declared factorized approximation; [Minka's EP paper](https://tminka.github.io/papers/ep/minka-ep-uai.pdf)
uses the opposite projection direction. Those results do not automatically
certify this mixed schedule, damping, finite arithmetic or truncated budgets.
No forward Exp/Log-to-Gamma projection is silently required by the fixture.

At fixed weights, the conditional product's normalizer is constant for the
local variational coordinate update. It generally depends on weights and
inputs when training a normalized conditional model. The proposed local
supervised fitting therefore remains a declared surrogate objective, with no
joint maximum-likelihood claim. Frozen point weights are learned parameters;
they do not become a Bayesian weight posterior through graph placement.

## Application and epoch invariants

The site law is essential: if B is the retained Gaussian base and q-star is the
projected Gaussian belief, use the checked natural-parameter quotient
`s_new=q-star/B`. Replace the old unary site; never multiply q-star as if it
were an independent site. Preserve the actual base-to-target rounding and the
certificate's exact rendered target. Even alpha=1 reconstruction can round, so
certificate scope must not silently transfer to different reconstructed bits.

For `0<alpha<=1`, natural interpolation combines old and proposed site
coefficients. A finite improper site is permissible; the resulting complete
belief still needs checked proper moments. At alpha below one, properness of
the applied belief is separate from certification of the undamped minimizer.
Gamma interpolation must use `LogPower,Rate`, retaining represented-shape
admission and encoding drift. The unchecked Gaussian operators are not that
admission boundary.

The block-entry snapshot, declared private dependencies and atomic replacement
make the proposed Gauss-Seidel schedule unambiguous at the design level. A
failed later block retains the earlier committed block and all actual returns.
Finite operation order, old-site identity and input revision are part of the
query, rather than properties inferred from a canonical evidence union.

Evidence identity, shared-prior ownership and parameter-artifact lineage remain
separate. Different producer paths or weight hashes cannot manufacture an
independent observation. A frozen query consumes one complete nested version
manifest. A training epoch proposes a new immutable artifact under an expected
parent revision, with selection only at the next query boundary. Retraction
replays an active cut from a retained checkpoint when descendants exist; it
neither negates an SGD step nor assumes floating subtraction reverses inference.

One concrete implementation constraint is worth retaining. Existing
[SoftScheduler.drive](https://github.com/Lucent-Financial-Group/Zeta/blob/15c634e096674c757f3656c0b8ff9aaa0b5cf203/src/Core/SoftScheduler.fs#L66)
returns only `Error e` after a handler refusal; it does not return its local
threaded state. The new adapter must preserve EpochResult, actual returns and
last committed state independently before that error path, or explicitly carry
a failed epoch as retained state under a declared stop rule. M4 must exercise
the chosen actual scheduler route. This is a limit of reuse, not a new
prerequisite or an error in the proposal's stated new-ledger requirement.

## Two analytic instances of the existing controls

These are independent paper derivations for M2/M3, not executed tests, generated
benchmark vectors or added prerequisite groups.

For M2, take a current z marginal with mean 2 and variance 3, and clamped
`mu=-1`. The residual is 12 and the precision site is `(LogPower=1/2,Rate=6)`.
Substituting a cavity with mean 0 and variance 1 gives residual 2 and rate 1.
Both are finite/proper-compatible, so this discriminates the wrong input role
without relying on a numeric range failure. A Gamma(2,3) prior plus the correct
site gives represented shape 5/2 and rate 9 in exact arithmetic.

For M3, target `(t,u,k,c)=(1,3/2,-3/4,1)` has stationary Gaussian
`(m,v)=(-1/4,1/2)`: `exp(m+v/2)=1`, and both objective derivatives vanish.
The existing strictly convex Gaussian-family problem makes this its minimizer.
The base natural parameters are `(eta,t)=(3/2,1)`, the projected belief is
`(-1/2,2)`, and the required unary site is `(-2,1)`. Belief-as-site instead
produces `(1,3)`, visibly counting the base twice.

Starting from the neutral unary site and damping that proposal by alpha=1/2
gives applied natural parameters `(1/2,3/2)`, hence `(m,v)=(1/3,2/3)`. This is
proper but not the above minimizer. Its mean derivative is
`exp(2/3)-5/12 > 0`. The example separates the proposed certificate from a
false applied-minimizer label using the already selected model and rule.
Finite implementation tests must retain their actual arithmetic and certificate
outcomes; this derivation is not a claim that any native call returned these bits.

## Reuse and finite exit

The selected definitions support the census's qualified reuse statements:
FactorGraph is homogeneous, excludes the receiving factor and replaces its map;
PrecisionGateKernels supplies the checked scalar families; SignedProbitEp shows
a stored-site update pattern without the complete ledger; and the toy learner
actually changes 37 Gaussian weight marginals in ordered ADF, with its existing
old-posterior fallback and variance clamp. MultilayerBnn's Gaussian exactness
labels and ReferenceFrameFactorHeterarchy's separate family graphs must remain
scoped. Core topology multiplicities, scheduler ticks and abstract cost units
do not establish evidence independence, inner solver work or physical energy.

The eight existing control groups cover the important boundaries without
requiring full PGE or a second general inference framework. Their finite exit
is useful: a reviewed adapter, one actual learned artifact on a named fixture,
and one nested frozen query, followed by a preregistered learned comparison.
The neural learner remains explicit missing work; a static scalar oracle or
the narrow fixed-feature control cannot be relabeled as that intended module.

Before implementation/registration, select the already-required concrete
receipt and training caps, exact typed rule/port records and finite source of
operations. Before fitting the comparison models, fix target/horizon and
as-of feature/label availability with chronological split/embargo semantics,
including inner cross-fitting. This makes the proposal's training-cut and
no-leakage requirements executable; split labels alone are insufficient.
No new dataset, precision range, optimizer theorem or broad PGE prerequisite
is requested by this review.

Individual, flat, shallow and deeper comparisons should use the same frozen
compatible artifact pool and allowed information, while counting their actual
work. Unchanged-model flattening is a separate control from topology that
changes the model. Best individual, equal fusion and a trained flat combiner
are useful finite controls. Failed fits, abstentions and uncertified queries
stay in denominators. A negative structural result ends the registered slice;
passing local certificates does not require or imply a structural win.

## Retained work and scope

The audit retains 17 lossless records, 83,389 raw / 34,864 stored bytes. Its first
local-only pass retained the proposal/census prefix, then exited 1 because the
prior report commit was unavailable in the inspected local Git stores. The
second pass used the exact immutable public raw GitHub URL and verified the
expected 5,567-byte report hash. That is a retrieval correction, not a proposal
finding or a numerical rerun. All 24 census identities were verified in the
completed pass. Every stored/decompressed record matches its local original.

The [prior report](https://raw.githubusercontent.com/Lucent-Financial-Group/Zeta/61f8e14ba9f89e790dce635ce5c86db5c619b63c/docs/research/2026-09-08-precision-gate-projection-registered-results.md)
supports the cancellation candidate / IterationLimit / NoRootEnclosure summary.
The row numbers are retained as the proposal's coordinator-supplied identities;
this review did not reopen or independently replay the raw 88-operation result.
It correctly follows that an uncertified proposal must apply zero new sites.

Work consisted of source/documentation reads, independent symbolic derivation,
metadata hashing, primary-literature inspection and documentation checks. The
identity helper and Git subprocesses ran; no Bayesian/reference module was
imported, and no solver, learner, scheduler, target stream, model deserialization,
benchmark or new experiment was executed. Original scalar outcome acceptance
remains a separate completed scope. Normal preservation-push checks are also
separate from a learning or numerical experiment.
