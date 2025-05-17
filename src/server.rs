use std::{
    collections::{HashMap, VecDeque},
    net::SocketAddr,
    ops::{ControlFlow, Deref},
    sync::Arc,
    time::Duration,
};

use axum::{
    Router,
    body::Bytes,
    extract::{
        ConnectInfo, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    response::{Html, IntoResponse},
    routing::get,
};
use axum_extra::{TypedHeader, headers, response::Wasm};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::{
    RwLock,
    mpsc::{Receiver, Sender},
};
use tower_http::trace::{DefaultMakeSpan, TraceLayer};

pub use tokio;
pub use tracing;
pub use tracing_subscriber;

use crate::dom::{DomNodeBuilder, DomNodeUnbuiltBody};

// pub type StateProcessorFnDyn<T> = dyn Fn(&mut T, SocketAddr, String) -> Option<Event> + Send + Sync;
// pub type ProcessorFnDyn<T> =
//     dyn Fn(&mut T, SocketAddr, serde_json::Value) -> Option<Event> + Send + Sync;

pub type StateProcessorFn<T> = fn(&mut T, SocketAddr, String) -> Option<Event>;
pub type CookieProcessorFn<T> = fn(&mut T, SocketAddr, String, String) -> Option<Event>;
pub type ProcessorFn<T> = fn(&mut T, SocketAddr, serde_json::Value) -> Option<Event>;

struct ApiState<T: Send + Sync> {
    events_to_be_sent: RwLock<VecDeque<Event>>,
    connected_clients: RwLock<HashMap<SocketAddr, ConnectedClient>>,
    state_processor: RwLock<Option<Box<StateProcessorFn<T>>>>,
    cookie_processor: RwLock<Option<Box<CookieProcessorFn<T>>>>,
    processors: RwLock<Vec<ProcessorFn<T>>>,
    routes: RwLock<HashMap<String, String>>,
    state: RwLock<T>,
}

impl<T: Send + Sync> ApiState<T> {
    fn new(
        state_processor: Option<Box<StateProcessorFn<T>>>,
        cookie_processor: Option<Box<CookieProcessorFn<T>>>,
        processors: Vec<ProcessorFn<T>>,
        routes: HashMap<String, String>,
        state: T,
    ) -> Self {
        Self {
            events_to_be_sent: RwLock::new(VecDeque::new()),
            connected_clients: RwLock::new(HashMap::new()),
            state_processor: RwLock::new(state_processor),
            cookie_processor: RwLock::new(cookie_processor),
            processors: RwLock::new(processors),
            routes: RwLock::new(routes),
            state: RwLock::new(state),
        }
    }

    async fn send_to_server(&self, from: SocketAddr, event: ToServerEvent) {
        self.events_to_be_sent
            .write()
            .await
            .push_back(Event::ToServer { from, event });
    }

    async fn send_to_all_clients(&self, event: ToClientEvent) {
        self.events_to_be_sent
            .write()
            .await
            .push_back(Event::ToAllClients(event));
    }

    async fn send_to_specific_client(&self, who: SocketAddr, event: ToClientEvent) {
        self.events_to_be_sent
            .write()
            .await
            .push_back(Event::ToSpecificClient { who, event });
    }
}

struct ConnectedClient {
    who: SocketAddr,
    tx: Sender<ToClientEvent>,
    // rx: Receiver<Event>,
}

#[derive(Debug)]
pub enum Event {
    ToServer {
        from: SocketAddr,
        event: ToServerEvent,
    },
    ToAllClients(ToClientEvent),
    ToSpecificClient {
        who: SocketAddr,
        event: ToClientEvent,
    },
}

#[derive(Debug, Clone)]
pub struct UserContext {}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ToServerEvent {
    Test(String),
    PageLoad { path: String, params: String },
    RequestFullState { name: String },
    Cookie { name: String, value: String },
    Custom(serde_json::Value),
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ToClientEvent {
    Alert {
        msg: String,
    },

    #[serde(rename_all = "camelCase")]
    DomUpdate {
        dom_id: String,
        html: String,
    },

    #[serde(rename_all = "camelCase")]
    RenderComponent {
        component_name: String,
        params: Option<String>,
        dom_id: Option<String>,
    },

    Custom {
        event: serde_json::Value,
    },
}

#[derive(Default)]
pub struct App<T: Default> {
    state_processor: Option<Box<StateProcessorFn<T>>>,
    cookie_processor: Option<Box<CookieProcessorFn<T>>>,
    processors: Vec<ProcessorFn<T>>,
    routes: HashMap<String, String>,
    wasm: Option<&'static [u8]>,
    state: T,
}

impl<T: Default + Send + Sync + 'static> App<T> {
    pub fn state_processor(mut self, f: StateProcessorFn<T>) -> Self {
        self.state_processor = Some(Box::new(f));
        self
    }

    pub fn cookie_processor(mut self, f: CookieProcessorFn<T>) -> Self {
        self.cookie_processor = Some(Box::new(f));
        self
    }

    pub fn add_processor(mut self, f: ProcessorFn<T>) -> Self {
        self.processors.push(f);
        self
    }

    pub fn route(mut self, path: &str, component_name: &str) -> Self {
        self.routes
            .insert(path.to_string(), component_name.to_string());
        self
    }

    pub fn state(mut self, state: T) -> Self {
        self.state = state;
        self
    }

    pub fn wasm(mut self, blob: &'static [u8]) -> Self {
        self.wasm = Some(blob);
        self
    }

    pub async fn serve(self) -> Result<(), Box<dyn std::error::Error>> {
        let state = Arc::new(ApiState::new(
            self.state_processor,
            self.cookie_processor,
            self.processors,
            self.routes.clone(),
            self.state,
        ));

        let cloned_state = state.clone();
        tokio::spawn(async move {
            let state = cloned_state;
            loop {
                let mut clients_to_remove =
                    Vec::with_capacity(state.connected_clients.read().await.len());
                let mut pending_events = Vec::new();

                while let Some(event) = state.events_to_be_sent.write().await.pop_front() {
                    match event {
                        Event::ToServer { from, event } => {
                            // TODO: send to wasm module endpoint
                            tracing::info!("look at me i'm totally a real wasm module {event:?}");

                            tracing::debug!("event: {event:?}");

                            match event {
                                ToServerEvent::Test(_) => {}
                                ToServerEvent::RequestFullState { name } => {
                                    tracing::info!("{from} is requesting full state {name}");
                                    if let Some(state_processor) =
                                        state.state_processor.read().await.deref()
                                    {
                                        let mut state = state.state.write().await;
                                        if let Some(event) = state_processor(&mut state, from, name)
                                        {
                                            pending_events.push(event);
                                        }
                                    } else {
                                        tracing::error!("no state processor registered");
                                    }
                                }
                                ToServerEvent::Cookie { name, value } => {
                                    if let Some(cookie_processor) =
                                        state.cookie_processor.read().await.deref()
                                    {
                                        let mut state = state.state.write().await;
                                        if let Some(event) =
                                            cookie_processor(&mut state, from, name, value)
                                        {
                                            pending_events.push(event);
                                        }
                                    } else {
                                        tracing::error!("no cookie processor registered");
                                    }
                                }
                                ToServerEvent::PageLoad { path, params } => {
                                    if let Some(component_name) =
                                        state.routes.read().await.get(&path)
                                    {
                                        pending_events.push(Event::ToSpecificClient {
                                            who: from,
                                            event: ToClientEvent::RenderComponent {
                                                component_name: component_name.clone(),
                                                params: Some(params.clone()),
                                                dom_id: Some("test".to_string()),
                                            },
                                        });
                                    }
                                }
                                ToServerEvent::Custom(value) => {
                                    let mut user_state = state.state.write().await;

                                    for processor in state.processors.read().await.iter() {
                                        // TODO: async?
                                        if let Some(event) =
                                            processor(&mut user_state, from, value.clone())
                                        {
                                            pending_events.push(event);
                                        }
                                    }
                                }
                            }
                        }
                        Event::ToAllClients(to_client_event) => {
                            // tracing::debug!("sending ToAllClients event {to_client_event:?}");

                            let mut clients = state.connected_clients.write().await;
                            for (who, client) in clients.iter_mut() {
                                if client.tx.send(to_client_event.clone()).await.is_err() {
                                    tracing::error!(
                                        "failed to send ToAllClients event to client {:?}",
                                        client.who
                                    );
                                    clients_to_remove.push(*who);
                                }
                            }
                        }
                        Event::ToSpecificClient { who, event } => {
                            if let Some(client) =
                                state.connected_clients.write().await.get_mut(&who)
                            {
                                if client.tx.send(event.clone()).await.is_err() {
                                    tracing::error!(
                                        "failed to send ToAllClients event to client {:?}",
                                        client.who
                                    );
                                    clients_to_remove.push(who);
                                }
                            }
                        }
                    }
                }

                state
                    .events_to_be_sent
                    .write()
                    .await
                    .append(&mut pending_events.into());

                for who in clients_to_remove.into_iter().rev() {
                    state.connected_clients.write().await.remove(&who);
                }

                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        });

        let mut component_routes = Router::new();
        for (path, _) in self.routes {
            component_routes = component_routes.route(&path, get(index));
        }

        let app = Router::new()
            .route(
                "/client.wasm",
                get(move || async move {
                    Wasm(Bytes::from(self.wasm.expect("wasm blob not provided")))
                }),
            )
            .route("/ws", get(ws_handler))
            .merge(component_routes)
            .layer(
                TraceLayer::new_for_http()
                    .make_span_with(DefaultMakeSpan::default().include_headers(true)),
            )
            .with_state(state.clone());

        let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
        tracing::debug!("listening on {}", listener.local_addr()?);

        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await?;

        Ok(())
    }
}

async fn index<T: Send + Sync>(State(state): State<Arc<ApiState<T>>>) -> Html<String> {
    let builder = DomNodeBuilder::default().push("html", || {
        DomNodeBuilder::default()
            .push("head", || {
                DomNodeBuilder::default()
                    .push("title", || "Hello World this is different".into())
                    .push("script", || include_str!("html/runtime.js").into())
                    .attr("type", "text/javascript")
                    .push("link", || "".into())
                    .attr("rel", "stylesheet")
                    .attr(
                        "href",
                        "https://cdn.jsdelivr.net/npm/@picocss/pico@2/css/pico.min.css",
                    )
            })
            .push("body", || {
                DomNodeBuilder::default()
                    .push("p", || "Disconnected".into())
                    .attr("id", "status")
            })
            .push("p", || "Loading wasm blob".into())
            .attr("id", "loading-text")
            .push("div", || "".into())
            .attr("data-pserve-id", "test")
    });

    Html(render(builder))
}

fn render(builder: DomNodeBuilder) -> String {
    let mut string = String::new();

    for builder in builder.children {
        {
            if !builder.tag.is_empty() {
                string.push_str(&format!("<{}", builder.tag));
            }

            for (attr, value) in &builder.attributes {
                if value.is_empty() {
                    string.push_str(&format!(" {attr} "));
                } else {
                    string.push_str(&format!(" {attr}='{value}' "));
                }
            }

            if !builder.tag.is_empty() {
                string.push('>');
            }
        }

        match &builder.body {
            Some(DomNodeUnbuiltBody::Text(text)) => string.push_str(text),
            Some(DomNodeUnbuiltBody::Constructor(ctor)) => {
                string.push_str(&render(ctor()));
            }
            None => {}
        }

        if !builder.tag.is_empty() {
            string.push_str(&format!("</{}>", builder.tag));
        }
    }

    string
}

// TODO: grab user context
async fn ws_handler<T: Send + Sync + 'static>(
    State(state): State<Arc<ApiState<T>>>,
    ws: WebSocketUpgrade,
    user_agent: Option<TypedHeader<headers::UserAgent>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> impl IntoResponse {
    let user_agent = if let Some(TypedHeader(user_agent)) = user_agent {
        user_agent.to_string()
    } else {
        String::from("Unknown browser")
    };
    println!("`{user_agent}` at {addr} connected.");

    let (events_to_socket, events_from_main_bus_rx) = tokio::sync::mpsc::channel(100);
    // let (events_to_main_bus_tx, events_from_socket_rx) = tokio::sync::mpsc::channel(100);

    state.connected_clients.write().await.insert(
        addr,
        ConnectedClient {
            who: addr,
            tx: events_to_socket,
            // rx: events_from_socket_rx,
        },
    );

    let state = state.clone();
    ws.on_upgrade(move |socket| handle_socket(socket, addr, state, events_from_main_bus_rx))
}

async fn handle_socket<T: Send + Sync + 'static>(
    socket: WebSocket,
    who: SocketAddr,
    state: Arc<ApiState<T>>,
    mut events_rx: Receiver<ToClientEvent>,
) {
    let (mut sender, mut receiver) = socket.split();

    let mut send_task = tokio::spawn(async move {
        while let Some(event) = events_rx.recv().await {
            sender
                .send(Message::Text(serde_json::to_string(&event).unwrap().into()))
                .await
                .unwrap();
        }
    });

    let state = state.clone();
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            if process_message(msg, who, &state).await.is_break() {
                break;
            }
        }
    });

    tokio::select! {
        rv_a = (&mut send_task) => {
            match rv_a {
                Ok(()) => println!("done sending to {who}"),
                Err(a) => println!("Error sending messages {a:?}")
            }
            recv_task.abort();
        },
        rv_b = (&mut recv_task) => {
            match rv_b {
                Ok(()) => println!("Done receiving messages"),
                Err(b) => println!("Error receiving messages {b:?}")
            }
            send_task.abort();
        }
    }

    println!("Websocket context {who} destroyed");
}

async fn process_message<T: Send + Sync>(
    msg: Message,
    who: SocketAddr,
    state: &ApiState<T>,
) -> ControlFlow<(), ()> {
    match msg {
        Message::Text(t) => {
            println!(">>> {who} sent str: {t:?}");
            if t.starts_with("alert") {
                state
                    .send_to_all_clients(ToClientEvent::Alert { msg: t.to_string() })
                    .await;
            } else if let Ok(value) = serde_json::from_str::<ToServerEvent>(&t) {
                tracing::info!("received ToServerEvent: {value:?}");
                state.send_to_server(who, value).await;
            } else if let Ok(value) = serde_json::from_str(&t) {
                tracing::info!("received Custom Event: {value:?}");
                state
                    .send_to_server(who, ToServerEvent::Custom(value))
                    .await;
            } else {
                println!(">>> {who} sent invalid json: {t:?}");
            }
        }
        Message::Binary(d) => {
            println!(">>> {} sent {} bytes: {:?}", who, d.len(), d);
        }
        Message::Close(c) => {
            if let Some(cf) = c {
                println!(
                    ">>> {} sent close with code {} and reason `{}`",
                    who, cf.code, cf.reason
                );
            } else {
                println!(">>> {who} somehow sent close message without CloseFrame");
            }
            return ControlFlow::Break(());
        }

        Message::Pong(v) => {
            println!(">>> {who} sent pong with {v:?}");
        }
        Message::Ping(v) => {
            println!(">>> {who} sent ping with {v:?}");
        }
    }
    ControlFlow::Continue(())
}
