-- Allow ordinary local accounts to create pubsub nodes.
--
-- Prosody only grants `pubsub:create-node` to `prosody:admin` by default, which
-- makes XEP-0503 spaces impossible on a shared pubsub component. This fixture
-- module mirrors a server where users may create their own nodes.

module:default_permission("prosody:registered", "pubsub:create-node");
