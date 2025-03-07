# Laika - Stream Processing with Event Correlation

Laika is a declarative stream processing engine that specializes in correlating and joining events from multiple data 
sources. It applies rules to these correlated events and takes actions when patterns match, all defined through YAML 
configuration.


Features
-----
* Event correlation: Group related events across different sources and formats using configurable keys
* Simple deployment: Single binary with minimal dependencies (Rust + CapnProto)
* Declarative configuration: Define event types, correlation rules, and actions in YAML
* Time-window processing: Schedule rule evaluation with configurable timing and windows
* JavaScript conditions: Use embedded JS for complex rule evaluation when needed


Laika bridges the gap between simple event processors and complex stream processing frameworks. 
It's designed for scenarios where you need to join events across streams and trigger actions based on event relationships,
without setting up complex distributed infrastructure.

## Quickstart
Build requirements are Rust and CapnpProto.

1. Save this file as "config.yaml":

```yaml
sources:
  local_file:
    type: File
    path: "./sample_events.jsonl"

targets:
  console_output:
    type: stdout

events:
  user_login:
    matchKey:
      event_type: "login"

triggers:
  filtered_login_events:
    requires:
      exact:
        - user_login
    action:
      target: targets.console_output
      payload:
        event: "new_login"
        userId: "${{ trigger.event.user_id }}"
        loginTime: "${{ trigger.event.timestamp }}"
        deviceInfo: "${{ trigger.event.device || 'unknown' }}"
```

2. Create a sample input file named "sample_events.jsonl":

```json lines
{"event_type": "login", "user_id": "user123", "timestamp": "2023-03-15T14:30:00Z", "device": "mobile"}
{"event_type": "page_view", "user_id": "user123", "timestamp": "2023-03-15T14:32:00Z", "page": "/products"}
{"event_type": "button_click", "user_id": "user123", "timestamp": "2023-03-15T14:33:00Z", "element_id": "add-to-cart"}
```

3. Run Laika with (WARN: This doesn't work yet):

```shell
cargo run -- --config=config.yaml
```

You should see the following output:
```json
{"event":"new_login","userId":"user123","loginTime":"2023-03-15T14:30:00Z","deviceInfo":"mobile"}
```

## Concepts

### Sources
Sources define where your input events come from. Laika can connect to various event sources including files, message queues, and HTTP endpoints.

```yaml
sources:
  rabbitmq_input:
    type: RabbitMQ
    connection: "amqp://guest:guest@localhost:5672"
    queue: "incoming_events"
```

### Event Typing
Events are tagged with user-defined "types" using Matchers. These types make it easier to process events according to their characteristics rather than their source.

If a user accepts three arbitrary events:
```json lines
{"a": 1}
{"a": 2}
{"b": 3}
```

We could divide the events into two `types` by applying matchers:

```yaml
events:
  typeA:
    matchKey:
      a: "*"  # Match any event with an "a" key
  typeB:
    matchKey:
      b: "*"  # Match any event with a "b" key
```

### Event Correlation
To process related events together, Laika lets you correlate events using keys. This divides your stream into logical partitions.

```yaml
correlation:
  eventA:
    key: "$.transaction_id"
  eventB:
    key: "$.txn_id"
  eventC:
    key: "$.transaction_id"
```

This configuration correlates events by their transaction ID (even though eventB uses a different field name), allowing you to make decisions based on groups of related events.

### Rule Requirements & Conditions

#### Requirements
Requirements define which event types must be present before a rule is evaluated:

```yaml
triggers:
  successCase:
    requires:
      exact:
        - eventA
        - eventB
        - eventC
```

The rule `successCase` is evaluated when exactly eventA, eventB, and eventC are present for any one grouping. This is the rule's requirement - what must happen before the rule's condition is checked.

#### NonCorrelated Rules
Rules that operate on individual events without needing to correlate them:

```yaml
triggers:
  detectSpam:
    requires:
      exact:
       - message
    filterAndExtract: >
      (trigger, ctx) => {
        const message = trigger.type === "received_event" ? trigger.event : null;
        if (!message || !message.content) return null;
        
        // Check for spam indicators
        const isSpam = message.content.includes("buy now") || 
                      message.content.includes("limited time offer");
        
        return isSpam ? { 
          messageId: message.id,
          userId: message.user_id,
          content: message.content 
        } : null;
      }
```

### Rules
Rules (defined under `triggers:` in YAML) specify when Laika should take action. Each rule has requirements (which events must be present) and optionally conditions (JavaScript expressions to evaluate).

#### Simple Rule
```yaml
triggers:
  detectNewUser:
    requires:
      exact:
        - login
    action:
      target: targets.welcomeNotification
      payload: 
        userId: "$.user_id"
        message: "Welcome to our platform!"
```

#### Correlated Rule with Condition
```yaml
triggers:
  purchaseAfterLogin:
    requires:
      exact:
        - login
        - purchase
    filterAndExtract: >
      (trigger, ctx) => {
        // trigger contains the current event that triggered the rule evaluation
        // ctx contains all previous events (excluding the current triggering event)
        const login = ctx.events.login[0];
        const purchase = trigger.type === "received_event" && trigger.event.type === "purchase" 
          ? trigger.event 
          : ctx.events.purchase ? ctx.events.purchase[0] : null;
          
        if (!purchase) return null; // Return null/undefined to not trigger the rule
        
        const conversionTime = duration(purchase.time, login.time);
        if (conversionTime > minutes(30)) return null;
        
        // Return an object with the data you want to use in the payload template
        return {
          userId: purchase.user_id,
          conversionTime: conversionTime,
          purchaseAmount: purchase.amount
        };
      }
    action:
      target: targets.analytics
      payload: 
        metric: "conversion"
        userId: "${{ userId }}"
        timeToConvert: "${{ conversionTime }}"
        revenue: "${{ purchaseAmount }}"
```

### Rule Filtering and Extraction
A filterAndExtract is a combined conditional and mapping - similar to `filter_map` in Rust. 
It is a JavaScript function that determines both whether a rule should trigger and what data to provide to the action payload.

```yaml
filterAndExtract: >
  (trigger, ctx) => {
    // Your logic here...
    
    if (someConditionFailed) return null; // Don't trigger
    
    return {
      // Data to use in payload template
      userId: user.id,
      calculatedValue: someCalculation
    };
  }
```

A condition function:
- Returns `null` or `undefined` to prevent the rule from triggering
- Returns an object with data to make it available for templating in the payload
- Has access to both the triggering event and the context of previous events

This approach allows you to combine the logic of "should this rule trigger?" with "what data should be included in the payload?" in a single function.

### Default Extract

When you don't specify a filterAndExtract for a rule, a default function is applied that prepares data for payload templates. 
This ensures all rules work consistently whether they have explicit function or not.

#### How Default Extractions Work

The default condition creates a structured object with the following properties:

```json
{
  "trigger": {
    "type": "received_event" // or "timer_expired",
    "timestamp": "Unix timestamp of when the trigger occurred",
    "event": { ... }  // Only present for received_event triggers
  },
  "events": {
    "eventType1": [ /* array of events, oldest first */ ],
    "eventType2": [ /* array of events, oldest first */ ]
  },
  "meta": {
    "eventType1_count": 1,  // Number of events of each type
    "eventType2_count": 3
  }
}
```

This structure provides access to:
- Information about what triggered the rule
- All events in the context, organized by type
- Metadata including counts for each event type

#### Accessing Data in Templates

You can access this data in your payload templates:

```yaml
triggers:
  paymentProcessing:
    requires:
      exact:
        - payment
    action:
      target: targets.paymentProcessor
      payload:
        # Trigger information
        triggerType: "${{ trigger.type }}"
        processedAt: "${{ trigger.timestamp }}"
        
        # When triggered by a payment event
        paymentId: "${{ trigger.event?.id }}"
        
        # First payment in the sequence (oldest)
        firstPaymentId: "${{ events.payment[0].id }}"
        
        # Most recent payment
        lastPaymentId: "${{ events.payment[meta.payment_count-1].id }}"
        
        # Total number of payments
        paymentCount: "${{ meta.payment_count }}"
```

For timer-triggered rules:

```yaml
triggers:
  dailyReport:
    requires:
      exact:
        - user_login
    timing:
      from: "24h"
      check_every: "24h"
    action:
      target: targets.reportingSystem
      payload:
        # Trigger information
        triggerType: "${{ trigger.type }}"  # Will be "timer_expired"
        reportTime: "${{ trigger.timestamp }}"
        
        # First and most recent login events
        firstLoginTime: "${{ events.user_login[0].time }}"
        lastLoginTime: "${{ events.user_login[meta.user_login_count-1].time }}"
        
        # Total count
        loginCount: "${{ meta.user_login_count }}"
```

#### Notes on Event Ordering

- Events in each array are ordered chronologically (oldest first)
- Use index `[0]` to access the oldest event of a type
- Use index `[meta.type_count-1]` to access the newest/most recent event

#### When to Use Custom Conditions

While the default condition works for simple cases, you should define a custom condition when you need to:
- Apply complex filtering logic
- Transform or combine data from multiple events
- Perform calculations before sending to the target
- Implement business logic specific to your use case

### Time-based Processing
Laika can also check rule conditions at specified intervals after the requirements are met. In this case, the `trigger` will have `{type: "timer_expired"}`:

```yaml
triggers:
  followUpReminder:
    requires:
      exact:
        - login
    timing:
      from: "30m"
      check_every: "30m"
      until: "4h"
    filterAndExtract: >
      (trigger, ctx) => {
        // ctx.sequence contains all previous events in the order they were received (excluding the current triggering event)
        // ctx.events is a dictionary mapping from event_type to list of events (excluding the current triggering event)
        
        // For timer-based triggers, we can check if there's been a purchase
        if (ctx.events.purchase && ctx.events.purchase.length > 0) {
          return null; // User already made a purchase, don't send reminder
        }
        
        // Return data for the payload
        return {
          userId: ctx.events.login[0].user_id,
          loginTime: ctx.events.login[0].time,
          elapsedTime: duration(new Date(), ctx.events.login[0].time)
        };
      }
    action:
      target: targets.notificationService
      payload:
        userId: "${{ userId }}"
        message: "Don't forget to check out our latest offers! You've been browsing for ${{ elapsedTime }} minutes."
```

### Actions and Payloads
When a rule's condition function returns a non-null value, Laika sends a payload to the specified target. Payloads support templating to access data returned from the condition function.

```yaml
action:
  target: targets.notificationSystem
  payload:
    user: "${{ userId }}"
    message: "Thanks for your purchase of ${{ purchaseAmount }}!"
    items: "${{ purchaseItems }}"
```

The variables in `${{ }}` are resolved using the data returned from the condition function.

### Targets
Targets define where actions send their results. Laika supports multiple output destinations:

```yaml
targets:
  notifications:
    type: RabbitMQ
    connection: "amqp://guest:guest@localhost:5672"
    queue: "notifications"
  
  auditLog:
    type: HTTP
    url: "https://api.example.com/audit"
    headers:
      Content-Type: "application/json"
      Authorization: "Bearer ${ENV_TOKEN}"
```

## Performance and Scaling

Laika is designed to be scalable and performant:

- Multiple instances can process different event keys in parallel without coordination
- Uses RocksDB for efficient storage with automatic key eviction
- Processing speed typically exceeds stream ingestion rates, making network the primary bottleneck

For high-volume scenarios, distribute events across multiple Laika instances based on your correlation keys.

## Connectors (WARN: This doesn't work yet)

Laika supports these connectors:

### RabbitMQ
```yaml
sources:
  rabbitmq_input:
    type: RabbitMQ
    connection: "amqp://guest:guest@localhost:5672"
    queue: "incoming_events"
    prefetch: 100  # Optional: number of messages to prefetch
```

### HTTP
```yaml
targets:
  api_endpoint:
    type: HTTP
    url: "https://api.example.com/events"
    method: "POST"  # Optional: defaults to POST
    headers:        # Optional
      Content-Type: "application/json"
      Authorization: "Bearer ${API_TOKEN}"
    retry:          # Optional
      attempts: 3
      backoff: "exponential"
```

### File
```yaml
sources:
  local_file:
    type: File
    path: "./input.jsonl"
    watch: true  # Optional: continue watching file for new content
```

More connectors will be added in future releases.