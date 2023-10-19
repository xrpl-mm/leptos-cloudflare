#![feature(result_flattening)]

mod app;
mod utils;

#[cfg(feature = "ssr")]
#[worker::event(fetch)]
pub async fn main(
    req: worker::Request,
    env: worker::Env,
    _ctx: worker::Context,
) -> worker::Result<worker::Response> {
    use std::collections::HashSet;
    use std::{net::SocketAddr, str::FromStr};

    use app::App;
    use leptos::*;
    use leptos_cf::{self, LeptosRoutes};
    use utils::set_panic_hook;
    use worker::Router;

    // Automatic registration of server functions doesn't work for wasm32 server
    utils::handle_register_server_fn(app::GetPost::register_explicit());
    utils::handle_register_server_fn(app::ListPostMetadata::register_explicit());

    set_panic_hook();

    let routes = leptos_cf::generate_route_list(|cx| view! { cx,  <App /> }.into_view(cx));

    // Manually specify options, because worker doesn't have access to local fs
    let leptos_options = LeptosOptions {
        output_name: String::from("example"),
        site_root: String::from("target/site"),
        site_pkg_dir: String::from("pkg"),
        env: leptos_config::Env::DEV,
        site_addr: SocketAddr::from_str("127.0.0.1:3000").unwrap(),
        reload_port: 3001,
    };

    let router = Router::with_data(leptos_cf::WorkerRouterData {
        options: leptos_options.clone(),
        static_dirs: HashSet::from([String::from("static"), String::from("css")]),
        app_fn: app::App,
    });

    worker::console_debug!("Routes: {:?}", routes);

    router
        .leptos_routes(routes)
        .get_async(
            &format!("/{}/:client_asset", &leptos_options.site_pkg_dir),
            leptos_cf::serve_static_from_kv,
        )
        .get_async("/static/:asset", leptos_cf::serve_static_from_kv)
        .post_async("/api/:fn_name", leptos_cf::handle_server_fns)
        .run(req, env)
        .await
}

#[cfg(feature = "ssr")]
fn log_request(req: &worker::Request) {
    worker::console_log!(
        "{} - [{}], located at: {:?}, within: {}",
        worker::Date::now().to_string(),
        req.path(),
        req.cf().coordinates().unwrap_or_default(),
        req.cf().region().unwrap_or_else(|| "unknown region".into())
    );
}

fn main() {}
