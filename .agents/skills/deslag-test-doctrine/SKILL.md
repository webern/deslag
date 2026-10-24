---
name: deslag-test-doctrine
description: >
  Read this skill before adding, deleting or changing tests, how tests run in CI, how tests are
  selected, etc.
disable-model-invocation: false
user-invocable: true
llm_edit: false
---
# /deslag-test-doctrine

## Build Doctrine

North Star: tests should provide fast feedback on actual behavior of the system. They should not be
brittle or coupled to the repo layout, dependency versions, or project state. Tests should focus on
ensuring the product behaves correctly.

## Fast

Running the standard set of product tests should take no more than a few seconds. This should be the
standard set of tests that an LLM can use when editing product code such as configuration behavior,
adding a comment, refactoring, restructuring the product code, etc. Lint unit tests should fall
under this category. They may use the small, checked-in corpus, or target specific checked-in files,
but not the big corpus.

CI for most PRs that do not involve tooling or training changes should complete in a few minutes.
Only when training, big corpus changes, or tooling changes are involved should a PR CI run take
longer than a few minutes.

## Avoid Over-testing

LLMs tend to add tests for everything that they are currently working on. By it's nature, an LLM
cannot keep track of the big-picture. Repos like this one end up having thousands of low value
tests. Examples of low value tests are ones that lock in very specific behaviors that do not affect
the product, over-testing of tooling code, testing of repo scripts, etc.

However, I (the human owner) understand that writing these tests helps the LLM get to the right
answer. Therefore, what I am asking is that, at the end of the work, before opening a PR, we take a
step back and ask, are all of these tests necessary? Are they ensuring the product works correctly?
Are they costly? Are they worth their weight in total test time?

Based on the answers to those questions, delete the tests that are some combination of expensive,
brittle, and not ensuring the quality of deslag as a product.

## Segmenting

Tests should be clearly segmented and documented in a way that any agent entering this codebase can
understand which tests it should run to be maximally efficient. If running all the tests takes
minutes instead of seconds, and an agent doesn't understand what it should run and not run, then we
massively slow down project development.

An agent should understand whether it needs to run the slow tests or not. It should also understand
a progressive level of depth that it has available with tests. Fast tests while iterating, then
something deeper if necessary before opening a PR.
