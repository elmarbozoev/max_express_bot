use crate::models::ProductStatus;

pub async fn product_ready(track_code: &str) -> bool {
    let url: String = "http://www.107kapro.cn/index/index/search?no=".to_string() + track_code;

    let response: String = reqwest::get(url)
        .await.expect("ERROR: Could not reach an api")
        .text()
        .await.expect("ERROR: Could not get the text from an api");

    let product_status: ProductStatus = serde_json::from_str(&response).expect("ERROR: Could not deserialize an object");

    match product_status.code.as_str() {
        "0000" => true,
        _ => false
    }
}