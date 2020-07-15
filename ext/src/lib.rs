mod abi;

#[no_mangle]
pub extern "C" fn run() {
    let j = serde_json::json!({"hello": "world"});
    let address = "http://localhost:8080/hello/world";

    println!("Going to send {} to {}", j, address);

    // Execute http request
    println!("{:?}", abi::execute_request(
        http::Request::post(address)
            .header("hello", "world")
            .body(serde_json::to_vec(&j).unwrap()).unwrap()
    ))

}

