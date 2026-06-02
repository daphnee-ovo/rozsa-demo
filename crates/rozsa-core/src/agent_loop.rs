use crate::config::AgentLoopConfig;
use crate::events::AgentEvent;
use crate::messages::AgentMessage;
use rozsa_model::event_stream::{EventStream, EventStreamSender, create_event_stream};
use tokio_util::sync::CancellationToken;

pub fn agent_loop(
    prompts: Vec<AgentMessage>,
    config: AgentLoopConfig,
    signal: Option<CancellationToken>,
) -> EventStream<AgentEvent> {
    let (sender, stream) = create_event_stream();
    tokio::spawn(async move {
        run_loop(prompts, config, sender, signal).await;
    });
    stream
}

pub fn agent_loop_continue(
    config: AgentLoopConfig,
    signal: Option<CancellationToken>,
) -> EventStream<AgentEvent> {
    let (sender, stream) = create_event_stream();
    tokio::spawn(async move {
        run_loop(vec![], config, sender, signal).await;
    });
    stream
}

async fn run_loop(
    _prompts: Vec<AgentMessage>,
    _config: AgentLoopConfig,
    _emit: EventStreamSender<AgentEvent>,
    _signal: Option<CancellationToken>,
) {
    // TODO: 实现核心循环
    // 1. transform_context
    // 2. convert_to_llm
    // 3. stream_fn.call(model, context, options)
    // 4. 消费 StreamEvent → 产出 AgentEvent
    // 5. tool dispatch
    // 6. before/after tool call hooks
    // 7. should_stop_after_turn check
    // 8. steering/follow_up queue drain
    // 9. loop if needed
}
