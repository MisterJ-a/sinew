use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

use sinew_core::ToolDescriptor;

use crate::{store::DEFAULT_PLAN_MODE_PROMPT, GoalWorkflowState, ToolRunResult};

use super::context::AgentMode;

const GOAL_MODE_PROMPT: &str = r#"You are in Goal mode.

Rules:
- Work autonomously toward the objective across as many turns as needed.
- Do not treat one answer as the end of the goal unless the objective is genuinely complete.
- Do not repeat completed work. First orient from existing context, then continue from the next useful step.
- Use tools and make edits normally, like Act mode.
- Before deciding the goal is complete, audit the objective against the current workspace state.
- When the objective is truly complete, you MUST call update_goal with status "complete" before your final response.
- If the objective is not complete by the end of this turn, briefly report progress and the next step. The app will continue automatically."#;

/// How the final plan is written and shaped. Owned by Sinew, appended after
/// the (user-editable) plan mode prompt: the plan file is what the user
/// reviews and what an implementation with a cleared context runs from.
const PLAN_FORMAT_PROMPT: &str = r#"When you write the final plan (only after the user clicked "Send and stop questions"), {SAVE}. Sinew saves it as the plan file: the user reviews and edits it there, and the implementation, possibly in a new conversation with no memory of this one, runs from it alone. So do not leave anything only in the chat. After saving, end with one short sentence.

Use this Markdown structure:

# <Title>

## Goal
What and why, in a few lines.

## Decisions
The choices agreed during the questions, one per line.

## Plan
What the system should do and how it behaves, for the user. No code.

## Steps
An ordered checklist the implementation follows, 3 to 12 steps, each one finished and checkable on its own:
- [ ] <step> — done when <an observable check: a test passes, a screen shows X, a command succeeds>

## Implementation notes
For the agent that implements this with a cleared context: the files and modules to change, existing functions or components to reuse, commands to run, risks and edge cases, and how to verify the whole. Concrete paths and names belong here, and only here, whatever other instructions say about implementation details."#;

pub(super) fn plan_format_prompt(save: &str) -> String {
    PLAN_FORMAT_PROMPT.replace("{SAVE}", save)
}

pub(super) const WRITE_PLAN_TOOL: &str = "write_plan";

pub(super) fn write_plan_descriptor() -> ToolDescriptor {
    ToolDescriptor {
        name: WRITE_PLAN_TOOL.into(),
        description: "Save the final plan as the plan file the user reviews and the implementation runs from. Call it once, with the complete plan in the required Markdown structure, only after the user clicked \"Send and stop questions\".".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "markdown": {
                    "type": "string",
                    "description": "The complete plan: Goal, Decisions, Plan, Steps (a - [ ] checklist), Implementation notes."
                }
            },
            "required": ["markdown"],
            "additionalProperties": false
        }),
    }
}

pub(super) fn run_write_plan(input: Value) -> ToolRunResult {
    let markdown = input
        .get("markdown")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default();
    if markdown.is_empty() {
        return ToolRunResult::err("markdown is required", Vec::new());
    }
    ToolRunResult::ok(
        "Plan saved. The user will review it before implementing; end your turn with one short sentence.",
        Vec::new(),
    )
}

pub(super) fn update_goal_descriptor() -> ToolDescriptor {
    ToolDescriptor {
        name: "update_goal".into(),
        description: "Mark the active Goal mode objective complete. Use this only after auditing that the full objective is genuinely finished.".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "status": {
                    "type": "string",
                    "enum": ["complete"],
                    "description": "Use complete only when the goal is truly done."
                },
                "summary": {
                    "type": "string",
                    "description": "A concise summary of what was completed."
                }
            },
            "required": ["status"],
            "additionalProperties": false
        }),
    }
}

pub(super) fn run_update_goal(
    goal_workflow: &mut GoalWorkflowState,
    input: Value,
) -> ToolRunResult {
    let status = input
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if status != "complete" {
        return ToolRunResult::err("status must be complete", Vec::new());
    }

    let Some((objective, started_at_ms)) = goal_objective_and_started(goal_workflow) else {
        return ToolRunResult::err("no active goal to update", Vec::new());
    };
    *goal_workflow = GoalWorkflowState::Complete {
        objective,
        started_at_ms,
        completed_at_ms: now_ms(),
    };

    let summary = input
        .get("summary")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Goal marked complete");
    ToolRunResult::ok(summary.to_string(), Vec::new())
}

fn goal_objective_and_started(goal_workflow: &GoalWorkflowState) -> Option<(String, i64)> {
    match goal_workflow {
        GoalWorkflowState::Active {
            objective,
            started_at_ms,
            ..
        }
        | GoalWorkflowState::Paused {
            objective,
            started_at_ms,
            ..
        }
        | GoalWorkflowState::Complete {
            objective,
            started_at_ms,
            ..
        } => Some((objective.clone(), *started_at_ms)),
        GoalWorkflowState::Idle => None,
    }
}

pub(super) fn goal_objective(goal_workflow: &GoalWorkflowState) -> Option<&str> {
    match goal_workflow {
        GoalWorkflowState::Active { objective, .. }
        | GoalWorkflowState::Paused { objective, .. }
        | GoalWorkflowState::Complete { objective, .. } => Some(objective.as_str()),
        GoalWorkflowState::Idle => None,
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}

pub fn system_prompt_for_mode(base: &str, mode: AgentMode) -> String {
    system_prompt_for_mode_with_plan_prompt(base, mode, DEFAULT_PLAN_MODE_PROMPT)
}

pub fn system_prompt_for_mode_with_plan_prompt(
    base: &str,
    mode: AgentMode,
    plan_mode_prompt: &str,
) -> String {
    match mode {
        AgentMode::Act => base.to_string(),
        AgentMode::Plan => format!(
            "{base}\n\n<plan_mode>\n{}\n</plan_mode>\n\n<plan_format>\n{}\n</plan_format>",
            effective_plan_mode_prompt(plan_mode_prompt),
            plan_format_prompt("call the write_plan tool with the complete plan")
        ),
        AgentMode::Goal => format!("{base}\n\n<goal_mode>\n{GOAL_MODE_PROMPT}\n</goal_mode>"),
    }
}

/// Plan mode for a turn run by Claude Code: the same rules, with Claude
/// Code's own tools (AskUserQuestion, and ExitPlanMode to hand in the plan).
pub(super) fn plan_mode_prompt_for_claude_code(plan_mode_prompt: &str) -> String {
    let rules = effective_plan_mode_prompt(plan_mode_prompt)
        .replace("the Question tool", "the AskUserQuestion tool")
        .replace("Question tool", "AskUserQuestion tool")
        .replace("save it with the write_plan tool", "hand it in with the ExitPlanMode tool");
    format!(
        "<plan_mode>\n{rules}\n</plan_mode>\n\n<plan_format>\n{}\n</plan_format>",
        plan_format_prompt("call the ExitPlanMode tool with the complete plan as its `plan`")
    )
}

fn effective_plan_mode_prompt(plan_mode_prompt: &str) -> &str {
    let prompt = plan_mode_prompt.trim();
    if prompt.is_empty() {
        DEFAULT_PLAN_MODE_PROMPT
    } else {
        prompt
    }
}

pub(super) fn system_prompt_for_turn(
    base: &str,
    mode: AgentMode,
    goal_workflow: &GoalWorkflowState,
    plan_mode_prompt: &str,
) -> String {
    let prompt = system_prompt_for_mode_with_plan_prompt(base, mode, plan_mode_prompt);
    if mode != AgentMode::Goal {
        return prompt;
    }
    let Some(objective) = goal_objective(goal_workflow) else {
        return prompt;
    };
    format!("{prompt}\n\n<goal_objective>\n{objective}\n</goal_objective>")
}
