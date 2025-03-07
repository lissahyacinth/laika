 @0xc2a772c7a7cb83aa;

struct CorrelatedEvent {
  received @0 :Int64;  # Timestamp - int64 (milliseconds since epoch)
  correlationId @1 :Text;
  eventType @2 :Text;
  data @3 :Data;
}

struct CorrelatedEventBatch {
    events @0 :List(CorrelatedEvent);
}

struct NonCorrelatedEvent {
  received @0 :Int64; # Timestamp - int64 (milliseconds since epoch)
  eventId @1 : Text;
  eventType @2 :Text;
  data @3 :Data;
}

struct NonCorrelatedEventBatch {
    events @0 :List(NonCorrelatedEvent);
}
