use utoipa::OpenApi;
fn main() {
    // Extensions are held in hash maps by utoipa. Serialize through JSON's
    // ordered object map so repeated generation is byte-for-byte stable.
    let value = serde_json::to_value(bracel_starter::ApiDoc::openapi()).expect("OpenAPI value");
    println!(
        "{}",
        serde_json::to_string_pretty(&value).expect("serialize OpenAPI")
    );
}
