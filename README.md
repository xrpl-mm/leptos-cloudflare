# leptos-cloudflare

[Leptos SSR](https://leptos-rs.github.io/leptos/ssr/index.html) with [`workers-rs`](https://github.com/cloudflare/workers-rs) backend, deployed directly to Cloudflare using [wrangler](https://github.com/cloudflare/workers-sdk).

## Prerequisites

Install [`wrangler`](https://github.com/cloudflare/workers-sdk):

```console
npm i -g wrangler
```

Install [`wasm-pack`](https://rustwasm.github.io/wasm-pack/installer/):

```console
curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh
```

## Local development

Run inside `example` directory:

```console
wrangler dev
```

Then access the website on localhost.

## Deployment

Run inside `example` directory:

```console
wrangler deploy
```

inside `worker` directory. This will deploy [Workers Sites](https://developers.cloudflare.com/workers/configuration/sites/).

## How it works

Client-side rendered code in `lib.rs` gets compiled first by running `wasm-pack build --target=web -- --features hydrate --no-default-features`.

Then, everything related to server-side rendering in `main.rs` gets compiled by running `worker-build --release -- --features ssr --no-default-features --bin example`.

There is no need to run these commands manually; `wrangler dev` will run them as they are defined in `wrangler.toml`.

## KV storage file name

As with any websites, static files need to be stored somewhere, and Cloudflare offers KV storage as a solution.

For deployment, `wrangler` will derive hashes from worker site files and append it to the file name before the extension. For example, `client_bg.wasm` will become `client_bg.849eaf9261.wasm`.

However, under local development, the file name does not contain a hash. These discrepancies need to be handled. However, `workers-rs` does not offer such a functionality natively.

There's a related PR at https://github.com/cloudflare/workers-rs/pull/308 but it seems that Cloudflare team is not keen on merging the PR or even integrating this feature into `workers-rs` repository.

Therefore, a separate fork has been made at https://github.com/9oelM/workers-rs/commit/00def197b6be6cb43604c7de1fc58523e95b6c84 to install `worker-build` and to serve as a dependency of the `worker` directory.
