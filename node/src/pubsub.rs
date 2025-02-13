use crate::prelude::from_json;
use neon::prelude::*;
use std::sync::{Mutex, MutexGuard};
use stencila::{
    once_cell::sync::{Lazy, OnceCell},
    serde_json,
};

/// The Neon event queue to which published events will be sent
static CHANNEL: OnceCell<Channel> = OnceCell::new();

/// A JavaScript subscription
#[derive(Debug)]
pub struct JsSubscription {
    /// The topic that is subscribed to
    topic: String,

    /// The subscriber function
    subscriber: Root<JsFunction>,
}

/// A list of JavaScript subscriptions
static SUBSCRIPTIONS: Lazy<Mutex<Vec<JsSubscription>>> = Lazy::new(|| Mutex::new(Vec::new()));

/// Obtain the subscriptions store
pub fn obtain(cx: &mut FunctionContext) -> NeonResult<MutexGuard<'static, Vec<JsSubscription>>> {
    match SUBSCRIPTIONS.try_lock() {
        Ok(guard) => Ok(guard),
        Err(error) => cx.throw_error(format!(
            "When attempting to obtain subscriptions: {}",
            error
        )),
    }
}

/// Subscribe to a topic
pub fn subscribe(mut cx: FunctionContext) -> JsResult<JsUndefined> {
    let topic = cx.argument::<JsString>(0)?.value(&mut cx);
    let subscriber = cx.argument::<JsFunction>(1)?.root(&mut cx);

    let channel = cx.channel();
    if CHANNEL.set(channel).is_err() {
        // Ignore because it just means channel was already set
    }

    let mut subscriptions = obtain(&mut cx)?;
    subscriptions.push(JsSubscription { topic, subscriber });

    Ok(cx.undefined())
}

/// Unsubscribe from a topic
pub fn unsubscribe(mut cx: FunctionContext) -> JsResult<JsUndefined> {
    let topic = cx.argument::<JsString>(0)?.value(&mut cx);

    let mut subscriptions = obtain(&mut cx)?;
    subscriptions.retain(|subscription| subscription.topic != topic);

    Ok(cx.undefined())
}

/// Publish data for a topic
pub fn publish(mut cx: FunctionContext) -> JsResult<JsUndefined> {
    let topic = cx.argument::<JsString>(0)?.value(&mut cx);
    let json = cx.argument::<JsString>(1)?.value(&mut cx);

    bridging_subscriber(topic, from_json::<serde_json::Value>(&mut cx, &json)?);

    Ok(cx.undefined())
}

/// A subscriber that acts as a bridge between Rust events and Javascript events
/// (i.e. takes a Rust event and turns it into a Javascript one)
///
/// This function is called by Rust for ALL topics and it passes on events to
/// Node.js subscribers that have subscribed to the particular topic.
pub fn bridging_subscriber(topic: String, data: serde_json::Value) {
    // If the queue is not set then it means that there are
    // no subscribers and so no need to do anything
    if let Some(queue) = CHANNEL.get() {
        queue.send(move |mut cx| {
            let subscriptions = &*SUBSCRIPTIONS
                .lock()
                .expect("Unable to obtain subscriptions lock");

            for JsSubscription {
                topic: sub_topic,
                subscriber,
            } in subscriptions
            {
                if sub_topic == "*" || topic.starts_with(sub_topic) {
                    let callback = subscriber.to_inner(&mut cx);
                    let this = cx.undefined();
                    let json = serde_json::to_string(&data).expect("Unable to convert to JSON");
                    let args = vec![cx.string(&topic), cx.string(json)];
                    callback.call(&mut cx, this, args)?;
                }
            }
            Ok(())
        });
    }
}

/// Initialize the pubsub module by registering the `bridging_subscriber`
/// as a subscriber to all topics.
pub fn init(mut cx: FunctionContext) -> JsResult<JsUndefined> {
    if let Err(error) = events::subscribe("*", events::Subscriber::Function(bridging_subscriber)) {
        return cx.throw_error(format!("While attempting to initialize pubsub: {}", error));
    }
    Ok(cx.undefined())
}
