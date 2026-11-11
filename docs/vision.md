# Vision

hbots is a messaging app for personal Bots. One Exact client. One Rust gateway on the machine that holds the Bots, the Vault, and the Projects board.

The idea came from Threebot.

A Bot's work runs in a harness. T3 Code is the harness wired today. The gateway can grow other backends later. The client talks to the gateway and never holds a harness token.

A Bot's thread is created on that local T3 today. The [execution environment design](bot-execution-environment.md) lets Settings, and each Bot's config, choose a linked computer, and limits the model list to that computer. A durable child's sheet uses the same choice. It is not built yet.
