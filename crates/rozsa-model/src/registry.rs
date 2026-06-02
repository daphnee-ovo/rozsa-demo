//! Process-local registry for API protocol providers.

use std::collections::HashMap;
use std::sync::{LazyLock, RwLock};

use crate::event_stream::EventStream;
use crate::types::{Api, Context, Model, SimpleStreamOptions, StreamOptions};

pub use crate::stream::StreamEvent;

/// Provider implementation for one model API protocol.
pub trait ApiProvider: Send + Sync {
    /// Return the API protocol handled by this provider.
    fn api(&self) -> &Api;
    /// Stream a provider-specific request using full provider options.
    fn stream(
        &self,
        model: &Model,
        context: &Context,
        options: &StreamOptions,
    ) -> EventStream<StreamEvent>;
    /// Stream a request using unified simple options.
    fn stream_simple(
        &self,
        model: &Model,
        context: &Context,
        options: &SimpleStreamOptions,
    ) -> EventStream<StreamEvent>;
}

static REGISTRY: LazyLock<RwLock<HashMap<Api, Box<dyn ApiProvider>>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// Register or replace the process-local provider for its API protocol.
pub fn register_provider(provider: Box<dyn ApiProvider>) {
    let api = provider.api().clone();
    REGISTRY.write().unwrap().insert(api, provider);
}

/// Return the currently registered provider for an API protocol.
pub fn get_provider(api: &Api) -> Option<impl std::ops::Deref<Target = Box<dyn ApiProvider>> + '_> {
    let guard = REGISTRY.read().unwrap();
    if guard.contains_key(api) {
        Some(RegistryRef {
            _guard: guard,
            api: api.clone(),
        })
    } else {
        None
    }
}

struct RegistryRef {
    _guard: std::sync::RwLockReadGuard<'static, HashMap<Api, Box<dyn ApiProvider>>>,
    api: Api,
}

impl std::ops::Deref for RegistryRef {
    type Target = Box<dyn ApiProvider>;
    fn deref(&self) -> &Self::Target {
        self._guard.get(&self.api).unwrap()
    }
}
