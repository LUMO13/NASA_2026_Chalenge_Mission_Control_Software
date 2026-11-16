# Suggested honest demo — FPECS / ASTRA-MED

**Note:** This is a **walkthrough to try after successful compilation**, not a claim that the steps have already been tested. The desktop app uses simulated data and an in-memory repository.

## Scenario A: First run and medical telemetry

1. Run `cargo run -p desktop-gui`.
2. Open **7. Користувачі та екіпаж**. Use your own unique administrator login/password.
3. Create an astronaut such as `AST-101` (fictional name, role, callsign, suit, height and mass).
4. Create an additional `FlightSurgeon` account (or `CrewMember` linked to `AST-101`).
5. Return to the **Crew Dashboard**, log in as the medical user, and click **Крок симуляції**.
6. Open the astronaut card and inspect health parameters and the plotted heart-rate samples.

The generated records disappear when you close the app. No actual PostgreSQL writes happen.

## Scenario B: Simulated incident

1. Go to **9. Панель симулятора**, select an astronaut first on the dashboard.
2. Select **EMERGENCY HYPOXIA**, then click **Крок симуляції**.
3. Inspect the alert center for warning/critical indicators produced by the rules engine.
4. In **10. Синхронізація із Землею**, turn off the link, create further telemetry and inspect the local queue.
5. Re-enable the simulated link, then use **Імітувати відправлення пакетів**.

**Do not describe this as real NASA DSN communication or as a validated medical assessment.** The link state, acknowledgments, pressure values and clinical thresholds are simulated for educational demonstration.
