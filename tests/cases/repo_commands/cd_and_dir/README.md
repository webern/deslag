# Project

```sh
cd sub
make sub-test
make test
make nowhere
```

```sh
make -C sub sub-test
make -C sub test
```

```sh
git clone https://example.com/project
cd project
make never-checked
```

```sh
cd $HOME/project
make ci
make never-checked
```
