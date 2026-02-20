use packdms::api::ApiDoc;
use utoipa::OpenApi;

fn main() {
    let openapi = ApiDoc::openapi();
    let json = openapi.to_pretty_json().unwrap();
    println!("{}", json);
}
