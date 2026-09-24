# `redis-rb` / `redis-client` upgrade test plan

You can verify the `redis` and `redis-client` gems installed in the `gitlab-mailroom` image
by using the `./dev/mailroom/redis/test.sh` script in this repository.

The script requires:

- Docker running locally.
- A successful CNG pipeline triggered by a GitLab team member.

## Running the test

Run from the root of this repository:

```sh
./dev/mailroom/redis/test.sh <image-tag>
```

The image tag is derived from the branch name, with `/` and `.` replaced by `-`.
For example, for a Renovate branch `renovate/redis-client-0.x`, run:

```sh
./dev/mailroom/redis/test.sh renovate-redis-client-0-x
```

## Expected output

```
TEST 1: redis gem (redis-rb) version
redis (5.x.x)
TEST 2: redis-client gem version
redis-client (0.x.x)
TEST 3: redis-rb basic connectivity (SET/GET)
"set foo to \"bar\""
"OK"
"get value of foo"
"bar"
TEST 4: redis-client basic connectivity (PING)
PONG
```

Verify that the versions reported in TEST 1 and TEST 2 match the versions bumped in
the merge request.
