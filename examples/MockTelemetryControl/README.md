# Mock telemetry-control example

This isolated project demonstrates IAOT's single-declaration callback format
against synthetic methods only. Its deliberately impossible RVAs do not target
any production binary.

The four report providers model native functions in a synthetic
`MockAnoSDK.framework`. Each uses the ABI `std::int64_t(int)` and returns zero
before entering the mock SDK's lower-level dispatch routine. The mock report
enablement callback is a no-op, and the two mock transport callbacks are
controlled by the default-enabled **Disable Mock SendToSvr** checkbox under the
Account tab. Turning the checkbox off forwards both transports to their
generated original trampolines.

Generate and validate the project from the repository root:

```powershell
cargo run --manifest-path patcher/Cargo.toml -- validate examples/MockTelemetryControl/hooks.yaml
cargo run --manifest-path patcher/Cargo.toml -- generate examples/MockTelemetryControl/hooks.yaml
```

To execute the example, replace the fixture RVAs only with addresses from an
owned synthetic ARM64 test application whose functions use the documented
mock ABIs.
