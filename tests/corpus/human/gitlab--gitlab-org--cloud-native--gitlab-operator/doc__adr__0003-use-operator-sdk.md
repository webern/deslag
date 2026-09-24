# 3. Use operator-sdk

Date: 2020-06-11

## Status

Accepted

Consequence of [2. Use RedHat codebase](0002-use-rh-codebase.md)

## Context

RedHat has used `operator-sdk` to boostrap the development of an operator

## Decision

We adopt limited use of `operator-sdk` as an initial scaffolding method

## Consequences

We will have to review our use of `operator-sdk` moving forward. For the time being we will retain most of the infrastructure it has created 
in case we will decide to continue with that approach and need a clean upgrade path.
