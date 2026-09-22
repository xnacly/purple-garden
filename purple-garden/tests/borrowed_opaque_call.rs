use std::{collections::HashMap, sync::Mutex};

use purple_garden::{pg_pkg, GardenOpaque, GardenValue, Pg};

#[derive(GardenOpaque)]
struct Store(Mutex<HashMap<String, Profile>>);

#[derive(Clone, GardenValue)]
struct Profile {
    name: String,
    plan: String,
}

#[pg_pkg]
mod store {
    use super::{Profile, Store};

    pub fn put(store: &Store, key: String, value: Profile) {
        store.0.lock().unwrap().insert(key, value);
    }

    pub fn get(store: &Store, key: String, fallback: Profile) -> Profile {
        store.0.lock().unwrap().get(&key).cloned().unwrap_or(fallback)
    }
}

#[test]
fn checked_call_borrows_an_opaque_handle() {
    let mut program = Pg::new()
        .with_lib(&store::PACKAGE)
        .compile(br#"
            import "store"
            fn provision(cache: Foreign<Store>) Str {
                store.put(cache "user:42" { name: "Ada" plan: "pro" })
                let profile = store.get(cache "user:42" { name: "n/a" plan: "free" })
                profile.name
            }
        "#)
        .unwrap();

    let cache = Store(Mutex::new(HashMap::new()));
    let provision = program.function::<(&Store,), String>("provision").unwrap();
    assert_eq!(program.call(&provision, (&cache,)).unwrap(), "Ada");
    assert_eq!(cache.0.lock().unwrap()["user:42"].plan, "pro");
}
