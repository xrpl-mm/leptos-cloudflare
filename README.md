# leptos-cf

POC for Leptos SSR done in Cloudflare workers.

## Prerequisites

Install [`wrangler`](https://github.com/cloudflare/workers-sdk):

```console
npm i -g wrangler
```

Install [`wasm-pack`](https://rustwasm.github.io/wasm-pack/installer/):

```console
curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh
```

Install [`cargo-watch`](https://github.com/watchexec/cargo-watch). This is going to be used for local development:

```console
cargo install cargo-watch
```

## Local development

`wasm-pack` needs to be used to compile the hydrated part of the app for client side rendering.

`wrangler` offers [custom build command option](https://developers.cloudflare.com/workers/wrangler/custom-builds/), but it does not offer an exit hook for the build script, so automatic termination of the processes created within the script is impossible if it is used. This means the background process created in the script will keep running even when the build is done. But a continuous background process needs to be run to compile the client side rendered part of the app with `wasm-pack` as `app` or `client` changes. Therefore, just run this command in a separate terminal at the project root for now:

```console
./watch-and-build-wasm.sh
```

Note that `build.rs` also can't seem to be used as an alternative because it is internally triggered by a `cargo` command, but running `wasm-pack` 'waits for file lock on build directory' because it uses `cargo` too, so it will hang forever, not allowing `build.rs` to finish.

However, if you don't bother watching file changes under `app` or `client` directory, you don't have to run the above script, and simply restart `wrangler` every single time you change something there.

With the above command running in another terminal, run inside `worker` directory:

```console
wrangler dev
```

Then access the website on localhost.

## Deployment

Simply run:

```console
wrangler deploy
```

inside `worker` directory.

## KV storage file name

As with any websites, static files need to be stored somewhere, and Cloudflare offers KV storage as a solution.

For deployment, `wrangler` will derive hashes from worker site files and append it to the file name before the extension. For example, `client_bg.wasm` will become `client_bg.849eaf9261.wasm`.

However, under local development, the file name does not contain a hash. These discrepancies need to be handled. However, `workers-rs` does not offer such a functionality natively.

There's a related PR at https://github.com/cloudflare/workers-rs/pull/308 but it seems that Cloudflare team is not keen on merging the PR or even integrating this feature into `workers-rs` repository.

Therefore, a separate fork has been made at https://github.com/9oelM/workers-rs/commit/00def197b6be6cb43604c7de1fc58523e95b6c84 to install `worker-build` and to serve as a dependency of the `worker` directory.
