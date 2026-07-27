# Staging Lessons & Architectural Rules

- **Vite Default Styling Override:** When migrating from immediate-mode GUIs (`egui`) to Vite/Tauri, Vite's default browser styling completely overrides native application aesthetics. A strict CSS reset wiping default margins/padding and redefining custom panel/border variables is mandatory to achieve visual parity.
- **UI Layout Desync:** Do not assume backend IPC integration automatically resolves missing frontend DOM components. When migrating complex state machines, explicitly validate that all required input fields (e.g., Export Configurations) and native folder pickers are scaffolded in the DOM before declaring frontend feature parity complete.
