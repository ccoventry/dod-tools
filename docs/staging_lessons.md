# Staging Lessons Learned
*Temporary holding zone for newly harvested rules. Review entries here before manually moving them into quirks or architecture files.*

## 📥 Unsorted Gotchas (Pending Review)
- None. (Active workspace clear)

## Workflow & AI Protocols
- **Workflow Friction (IDE Agent Output Fragmentation):** To prevent fragmented responses, enforce a strict "TASK COMPLETION REPORT" in `.cursorrules` requiring the IDE AI to output a single markdown code block (`### 📡 Return Payload for Web AI`) containing modified files, logic changes, and terminal status.
- **Workflow Friction (Context Contamination):** Store all Web AI prompt templates in a `.ai_prompts/` hidden folder. `.cursorrules` must explicitly ignore hidden directories to prevent the IDE AI from reading meta-instructions as project code.
- **Workflow Friction (Markdown Escaping):** When crafting prompt templates, use standard text formatting or avoid nested markdown code blocks (e.g., ```markdown) to prevent breaking the IDE's UI rendering or the Web AI's output generation block.
- **IDE Predictive AI Interference:** The IDE's inline predictive AI (Ghost Text) heavily relies on pattern matching and will aggressively attempt to delete lines it perceives as breaking a list's pattern (e.g., meta-instructions). Always manually verify inline predictive diffs before accepting.
