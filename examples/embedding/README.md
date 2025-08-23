# Embedding purple garden

Since purple garden is a self contained scripting language,
it is intended to be embedded into larger applications for
things like configuration, plugin systems and extendablity.

This example showcases the following:

1. Building purple garden
2. Embedding purple garden (see the annotated source files)
3. Linking purple garden

## 1. Builiding purple garden

The easiest way to build pg into a static object is entering
the nix dev env:

```shell
nix develop .
```

And building the library:

```shell
make lib
```

This produces `build/libpg.a`.

## 3. Linking purple garden

```shell
cd examples/embedding
make all
./embedding
```
