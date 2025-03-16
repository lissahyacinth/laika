use clap::Parser;
use laika_combiner::EventProcessor;
use laika_combiner::action::EventAction;
use laika_combiner::config::EventProcessorConfig;
use laika_combiner::config::builder::EventProcessorYamlSpec;
use laika_combiner::connections::{AckCallback, Connections};
use laika_combiner::errors::LaikaResult;
use laika_combiner::event::RawEvent;
use laika_combiner::event_handler::{handle_raw_event, handle_timing_expiry};
use laika_combiner::storage::StorageKVBuilder;
use laika_combiner::timing::TimingExpiry;
use std::env::temp_dir;
use std::fs;
use std::path::Path;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[arg(short, long)]
    config: String,
}

async fn process(connections: Connections, mut processors: Vec<EventProcessor>) -> LaikaResult<()> {
    let mut waker = TimingExpiry::new(".timing_expiry".to_string().parse().unwrap()).unwrap();
    // TODO: Allow setting a real path for this with a config - otherwise running multiple sessions one after the other
    //  isn't viable. We also need to forbid trying to open the same StorageKV
    let tmp_path = temp_dir();
    let mut storage = StorageKVBuilder::new(tmp_path).build()?;
    while let Ok(messages) = connections.receive().await {
        tracing::debug!("Received {} message(s) from connections", messages.len());
        let mut event_actions: Vec<(Vec<EventAction>, Option<AckCallback>)> = Vec::new();
        for (message, message_source, callback) in messages {
            let resultant_actions = handle_raw_event(
                processors.as_mut_slice(),
                &mut storage,
                message_source.as_str(),
                RawEvent::new(message),
            )?;
            event_actions.push((resultant_actions, Some(callback)));
        }
        while let Some(expiry) = waker.peek() {
            let resultant_actions =
                handle_timing_expiry(processors.as_mut_slice(), &mut storage, expiry)?;
            event_actions.push((resultant_actions, None));
            // I wonder if the ACK here needs to be handled in the same way as message acks.
            waker.ack()?;
        }
        tracing::debug!("Processing {} actions", event_actions.iter().filter(|(e, _)| !e.is_empty()).count());
        for (message_actions, callback) in event_actions {
            if !message_actions.is_empty() {
                tracing::debug!("Processing {:?} action", &message_actions);    
            }
            for message_action in message_actions {
                match message_action {
                    EventAction::Emit(emit_action) => {
                        connections
                            .submit_to(emit_action.target.as_str(), emit_action.clone().payload())
                            .await?;
                    }
                    EventAction::ScheduleWakeup(wakeup) => {
                        waker.add_expiry(wakeup)?;
                    }
                }
            }
            if let Some(callback_fn) = callback {
                callback_fn().await?;
            }
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config_path = Path::new(&cli.config);
    if !config_path.exists() {
        eprintln!("Error: Config file '{}' does not exist", cli.config);
        std::process::exit(1);
    }
    let yaml_content = match fs::read_to_string(config_path) {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Error reading config file: {}", e);
            std::process::exit(1);
        }
    };

    let processor_spec: EventProcessorYamlSpec = match serde_yaml::from_str(&yaml_content) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("Could not read Config: {}", e);
            std::process::exit(1);
        }
    };

    let processor_config = match EventProcessorConfig::try_from(&processor_spec) {
        Ok(processor) => processor,
        Err(e) => {
            eprintln!("Config is not invalid: {}", e);
            std::process::exit(1);
        }
    };

    tracing::info!("Initialised with config {:?}", &processor_config);
    let connections = processor_config.connections().await.unwrap();
    tracing::info!("Initialised with connections {:?}", &connections);
    let processor: EventProcessor = processor_config.build();

    if let Err(e) = process(connections, vec![processor]).await {
        eprintln!("Processing failed: {}", e);
        std::process::exit(1);
    }
}
