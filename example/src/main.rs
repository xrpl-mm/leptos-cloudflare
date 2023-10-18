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
    use std::{net::SocketAddr, str::FromStr};

    use app::App;
    use leptos::*;
    use leptos_cf::{self, LeptosRoutes};
    use utils::set_panic_hook;
    use worker::Router;

    log_request(&req);

    match app::GetPost::register_explicit() {
        Ok(_) => worker::console_debug!("Registered GetPost"),
        // this will run this every single time a request is made,
        // but we only want to register once, so we ignore the error when it complains
        // about duplicate registrations
        Err(ServerFnError::Registration(_)) => {}
        Err(err) => panic!("Failed to register: {:?}", err),
    }
    match app::ListPostMetadata::register_explicit() {
        Ok(_) => worker::console_debug!("Registered ListPostMetadata"),
        // this will run this every single time a request is made,
        // but we only want to register once, so we ignore the error when it complains
        // about duplicate registrations
        Err(ServerFnError::Registration(_)) => {}
        Err(err) => panic!("Failed to register: {:?}", err),
    }
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
        app_fn: app::App,
    });

    worker::console_debug!("Routes: {:?}", routes);

    router
        .leptos_routes(routes)
        .get_async(
            &format!("/{}/:wasm_bindgen_asset", &leptos_options.site_pkg_dir),
            leptos_cf::serve_wasm_bindgen_assets,
        )
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
