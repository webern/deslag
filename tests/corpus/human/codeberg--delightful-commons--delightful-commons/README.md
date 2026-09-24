# delightful-club

The delightful curated list of delightful curated lists

## Build

Run:

```
cargo run
cd ./website
zola build
```

If you want to use docker, run:

```
docker run --rm -u "$(id -u):$(id -g)" -v $PWD:/app -w /app rust:1.54.0 cargo run
docker run --rm -u "$(id -u):$(id -g)" -v $PWD:/app -w /app/website balthek/zola:0.14.0 build
```

## Contribute

To add a new delightful list, append it at the end of the `config.toml` file and submit a PR.