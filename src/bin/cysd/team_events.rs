//! Fixed refresh notification; caller text never enters the event stream.
use super::events::EventBus;

pub fn publish(bus: &EventBus) {
    bus.publish("team.changed", "team", None, serde_json::json!({}));
}
