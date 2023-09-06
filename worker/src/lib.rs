use std::str::FromStr;

use app::*;
use futures::StreamExt;
use leptos::*;
use leptos_dom::ssr::render_to_stream;
use worker::*;

mod utils;

const KV_KEY_PREFIX: &str = "$__MINIFLARE_SITES__$";

struct TryFromKVKeyToFileExtError(String);

struct FileExt(String);

struct ContentType(String);

#[event(fetch)]
pub async fn main(req: Request, env: worker::Env, _ctx: worker::Context) -> Result<Response> {
    log_request(&req);

    utils::set_panic_hook();

    let router = Router::new();

    router.get("/", |_req: Request, _ctx| {
        let pkg_path = "/client/client";
        let head = format!(
            r#"<!DOCTYPE html>
            <html lang="en">
                <head>
                    <meta charset="utf-8"/>
                    <meta name="viewport" content="width=device-width, initial-scale=1"/>
                    <link rel="modulepreload" href="{pkg_path}.js">
                    <link rel="preload" href="{pkg_path}_bg.wasm" as="fetch" type="application/wasm" crossorigin="">
                    <script type="module">import init, {{ hydrate }} from '{pkg_path}.js'; init('{pkg_path}_bg.wasm').then(hydrate);</script>
                </head>
                <body>"#
        );

        let tail = "</body></html>";

        let stream = futures::stream::once(async move { head.clone() })
            .chain(render_to_stream(|cx| view! { cx,  <App /> }.into_view(cx)))
            .chain(futures::stream::once(async { tail.to_string() }))
            .inspect(|html| println!("{html}"))
            .map(|html| Result::Ok(html.into_bytes()));
        let mut response = Response::from_stream(stream)?;
        response
            .headers_mut()
            // Set the content type header
            .set("Content-Type", "text/html")?;
        Ok(response)
    }).get_async("/client/:resource", |_req, ctx| async move {
        let resource = match ctx.param("resource") {
            Some(resource) => resource,
            None => {
                return Ok(Response::from_bytes(b"Not found".to_vec())?.with_status(404))
            }
        };

        let store = ctx.env.kv("__STATIC_CONTENT")?;
        let file_name = &format!("{}{}{}", KV_KEY_PREFIX, "/", resource);
        if let Some(bytes) = store.get(file_name).bytes().await? {
            let mut response = Response::from_bytes(bytes)?;
            let file_ext = match FileExt::from_str(file_name) {
                Ok(file_ext) => file_ext,
                Err(msg) => {
                    return Response::error(msg.0, 500);
                }
            };
            let content_type = ContentType::from(file_ext).to_string();
            response
                .headers_mut()
                // Set the content type header
                .set("Content-Type", &content_type)?;
            Ok(response)
        } else {
            Ok(Response::from_bytes(b"Not found".to_vec())?.with_status(404))
                }
    }).run(req, env).await
}

fn log_request(req: &Request) {
    console_log!(
        "{} - [{}], located at: {:?}, within: {}",
        Date::now().to_string(),
        req.path(),
        req.cf().coordinates().unwrap_or_default(),
        req.cf().region().unwrap_or_else(|| "unknown region".into())
    );
}

impl FromStr for FileExt {
    type Err = TryFromKVKeyToFileExtError;

    fn from_str(file_name: &str) -> std::result::Result<Self, Self::Err> {
        // ASSUMPTION: file only has a single dot in the name
        match file_name.rsplit_once('.') {
            None => Err(TryFromKVKeyToFileExtError(format!(
                "Unable to extract ext or kv hash from file [{}]",
                file_name
            ))),
            Some(pair) => {
                let (_, ext) = pair;

                Ok(FileExt(ext.to_owned()))
            }
        }
    }
}

impl From<FileExt> for ContentType {
    fn from(file_ext: FileExt) -> Self {
        let content_type = match file_ext.0.as_str() {
            "html" => "text/html",
            "css" => "text/css",
            "js" => "text/javascript",
            "json" => "application/json",
            "png" => "image/png",
            "jpg" => "image/jpeg",
            "jpeg" => "image/jpeg",
            "ico" => "image/x-icon",
            "wasm" => "application/wasm",
            _ => "text/plain",
        };

        ContentType(content_type.to_owned())
    }
}

impl ToString for ContentType {
    fn to_string(&self) -> String {
        self.0.to_owned()
    }
}
