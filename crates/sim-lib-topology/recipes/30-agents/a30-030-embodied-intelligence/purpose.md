# Embodied Intelligence Agent

This recipe records deterministic embodied control as SIM data. The setup
quotes a synthetic mobile base, a slow fake-runner strategy loop, a fast
deterministic controller, a binary constraint envelope, and topology-level
failure propagation.

The fixture is local and synthetic. It performs no live actuation; the actuator
gate failure propagates to simulated motors and selects a stable safe state.
