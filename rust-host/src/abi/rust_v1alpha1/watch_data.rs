use crate::kube_watch::{WatchKey};
use serde::{Deserialize, Serialize};

// The following structs are copy-pasted/derived from kube-rs, but they implement ser/de

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Resource {
    /// The API version of the resource.
    ///
    /// This is a composite of Resource::GROUP and Resource::VERSION
    /// (eg "apiextensions.k8s.io/v1beta1")
    /// or just the version for resources without a group (eg "v1").
    /// This is the string used in the apiVersion field of the resource's serialized form.
    api_version: String,

    /// The group of the resource
    ///
    /// or the empty string if the resource doesn't have a group.
    group: String,

    /// The kind of the resource.
    ///
    /// This is the string used in the kind field of the resource's serialized form.
    kind: String,

    /// The version of the resource.
    version: String,

    /// The namespace if the resource resides (if namespaced)
    namespace: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct WatchParams {
    resource_version: String,
    field_selector: Option<String>,
    include_uninitialized: bool,
    label_selector: Option<String>,
    allow_bookmarks: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WatchRequest {
    pub resource: Resource,
    pub watch_params: WatchParams,
}

impl Into<WatchKey> for WatchRequest {
    fn into(self) -> WatchKey {
        let resource = kube::Resource {
            api_version: self.resource.api_version,
            group: self.resource.group,
            kind: self.resource.kind,
            version: self.resource.version,
            namespace: self.resource.namespace,
        };
        let list_params = kube::api::ListParams {
            field_selector: self.watch_params.field_selector,
            label_selector: self.watch_params.label_selector,
            timeout: None,
            allow_bookmarks: self.watch_params.allow_bookmarks,
            limit: None,
            continue_token: None,
        };
        let resource_version = self.watch_params.resource_version;
        WatchKey {
            resource_version,
            resource,
            list_params,
        }
    }
}
