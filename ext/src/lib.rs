use k8s_openapi::api::core::v1::Pod;
use kube::api::ListParams;
use kube::runtime::Reflector;
use kube::{Api, Client};

#[no_mangle]
pub extern "C" fn run() {
    let client = Client::default();

    let pods: Api<Pod> = Api::namespaced(client, "default");
    let lp = ListParams::default().timeout(1);
    let rf = Reflector::new(pods).params(lp);

    loop {
        rf.poll().expect("Poll error!");

        println!("Poll completed, pods in default:");

        for p in rf.state().expect("Cannot get reflector state") {
            println!("{:?}", p)
        }
    }
}
