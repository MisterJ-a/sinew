use super::*;

fn sample_plan_ready() -> PlanWorkflowState {
    PlanWorkflowState::PlanReady {
        artifact: PlanArtifactState {
            path: ".sinew/plans/test.md".into(),
            absolute_path: Some("/workspace/.sinew/plans/test.md".into()),
            title: Some("Test plan".into()),
            updated_at_ms: Some(1),
        },
    }
}

#[test]
fn plan_policy_starts_question_loop_from_idle_plan_mode() {
    let policy = plan_turn_policy(&PlanWorkflowState::Idle, AgentMode::Plan, None).unwrap();

    assert_eq!(policy.mode, AgentMode::Plan);
    assert!(!policy.stop_questions);
    assert!(!policy.attach_plan);
    assert_eq!(policy.next_workflow, PlanWorkflowState::PlanningQuestions);
}

#[test]
fn plan_policy_forces_plan_mode_while_questions_are_active() {
    let policy =
        plan_turn_policy(&PlanWorkflowState::PlanningQuestions, AgentMode::Act, None).unwrap();

    assert_eq!(policy.mode, AgentMode::Plan);
    assert_eq!(policy.next_workflow, PlanWorkflowState::PlanningQuestions);
}

#[test]
fn plan_policy_only_attaches_plan_after_stop_questions() {
    let policy = plan_turn_policy(
        &PlanWorkflowState::PlanningQuestions,
        AgentMode::Plan,
        Some(PlanControlInput::StopQuestions),
    )
    .unwrap();

    assert_eq!(policy.mode, AgentMode::Plan);
    assert!(policy.stop_questions);
    assert!(policy.attach_plan);
    assert_eq!(policy.next_workflow, PlanWorkflowState::PlanningQuestions);
}

#[test]
fn plan_policy_rejects_implementation_before_plan_exists() {
    let err = plan_turn_policy(
        &PlanWorkflowState::PlanningQuestions,
        AgentMode::Act,
        Some(PlanControlInput::ImplementPlan),
    )
    .unwrap_err();

    assert!(err.contains("create the plan"));
}

#[test]
fn plan_policy_allows_card_actions_after_manual_exit() {
    let update_policy = plan_turn_policy(
        &PlanWorkflowState::Idle,
        AgentMode::Act,
        Some(PlanControlInput::UpdatePlan),
    )
    .unwrap();
    assert_eq!(update_policy.mode, AgentMode::Plan);
    assert_eq!(
        update_policy.next_workflow,
        PlanWorkflowState::PlanningQuestions
    );

    let implement_policy = plan_turn_policy(
        &PlanWorkflowState::Idle,
        AgentMode::Act,
        Some(PlanControlInput::ImplementPlan),
    )
    .unwrap();
    assert_eq!(implement_policy.mode, AgentMode::Act);
    assert_eq!(implement_policy.next_workflow, PlanWorkflowState::Idle);
}

#[test]
fn plan_policy_rejects_act_mode_when_plan_is_ready_without_user_action() {
    let err = plan_turn_policy(&sample_plan_ready(), AgentMode::Act, None).unwrap_err();

    assert!(err.contains("choose update plan or implement plan"));
}

#[test]
fn plan_policy_allows_implementation_after_plan_is_ready() {
    let policy = plan_turn_policy(
        &sample_plan_ready(),
        AgentMode::Act,
        Some(PlanControlInput::ImplementPlan),
    )
    .unwrap();

    assert_eq!(policy.mode, AgentMode::Act);
    assert_eq!(policy.next_workflow, PlanWorkflowState::Idle);
    assert!(!policy.attach_plan);
}

#[test]
fn plan_policy_returns_to_question_loop_when_updating_ready_plan() {
    let policy = plan_turn_policy(
        &sample_plan_ready(),
        AgentMode::Plan,
        Some(PlanControlInput::UpdatePlan),
    )
    .unwrap();

    assert_eq!(policy.mode, AgentMode::Plan);
    assert_eq!(policy.next_workflow, PlanWorkflowState::PlanningQuestions);
}

#[test]
fn context_estimate_stays_in_plan_mode_for_active_workflows() {
    assert_eq!(
        plan_estimate_mode(&PlanWorkflowState::PlanningQuestions, AgentMode::Act),
        AgentMode::Plan
    );
    assert_eq!(
        plan_estimate_mode(&sample_plan_ready(), AgentMode::Act),
        AgentMode::Plan
    );
}

#[test]
fn rewritable_user_message_rejects_compaction_and_hidden_messages() {
    let normal = ChatMessage {
        role: Role::User,
        parts: vec![Part::Text {
            text: "change this".into(),
            meta: None,
        }],
    };
    let retained = ChatMessage {
        role: Role::User,
        parts: vec![Part::Text {
            text: "old user message".into(),
            meta: Some(json!({ "compaction_retained_user": true })),
        }],
    };
    let summary = ChatMessage {
        role: Role::User,
        parts: vec![Part::Text {
            text: "summary".into(),
            meta: Some(json!({ "compaction_summary": true })),
        }],
    };
    let reminder = ChatMessage {
        role: Role::User,
        parts: vec![Part::Text {
            text: "continue".into(),
            meta: Some(json!({ "system_reminder": true })),
        }],
    };

    assert!(is_rewritable_user_message(&normal));
    assert!(!is_rewritable_user_message(&retained));
    assert!(!is_rewritable_user_message(&summary));
    assert!(!is_rewritable_user_message(&reminder));
}

#[test]
fn plan_implementation_reminder_uses_ready_plan_artifact() {
    let reminder = plan_implementation_turn_reminder(
        Path::new("/workspace"),
        &sample_plan_ready(),
        &[],
        Some(PlanControlInput::ImplementPlan),
        &[],
    )
    .unwrap()
    .unwrap();

    assert!(reminder.contains("Plan path: .sinew/plans/test.md"));
    assert!(reminder.contains("Plan title: Test plan"));
    assert!(reminder.contains("current turn"));
    assert!(reminder.contains("Use the ToDoList tool"));
}

#[test]
fn plan_implementation_reminder_uses_attached_plan_after_context_clear() {
    let attachments = vec![AttachmentInput {
        path: "/workspace/.sinew/plans/fresh.md".into(),
        name: Some("fresh.md".into()),
    }];
    let reminder = plan_implementation_turn_reminder(
        Path::new("/workspace"),
        &PlanWorkflowState::Idle,
        &attachments,
        Some(PlanControlInput::ImplementPlan),
        &[],
    )
    .unwrap()
    .unwrap();

    assert!(reminder.contains("Plan path: .sinew/plans/fresh.md"));
}

#[test]
fn plan_implementation_reminder_is_scoped_to_implement_control() {
    let reminder = plan_implementation_turn_reminder(
        Path::new("/workspace"),
        &sample_plan_ready(),
        &[],
        Some(PlanControlInput::UpdatePlan),
        &[],
    )
    .unwrap();

    assert!(reminder.is_none());
}

#[test]
fn plan_implementation_reminder_includes_selected_skills() {
    let reminder = plan_implementation_turn_reminder(
        Path::new("/workspace"),
        &sample_plan_ready(),
        &[],
        Some(PlanControlInput::ImplementPlan),
        &["impeccable".to_string(), "  ".to_string(), "seo-vibe-code".to_string()],
    )
    .unwrap()
    .unwrap();

    assert!(reminder.contains("skill tool"));
    assert!(reminder.contains("impeccable, seo-vibe-code"));
}

#[test]
fn plan_implementation_reminder_omits_skill_line_when_none_selected() {
    let reminder = plan_implementation_turn_reminder(
        Path::new("/workspace"),
        &sample_plan_ready(),
        &[],
        Some(PlanControlInput::ImplementPlan),
        &[],
    )
    .unwrap()
    .unwrap();

    assert!(!reminder.contains("skill tool"));
}

#[test]
fn swarm_completion_event_extracts_structured_responses() {
    let event = AgentEvent::ToolFinished {
        id: "team-run".to_string(),
        output: "Agent Swarm finished".to_string(),
        is_error: false,
        file_changes: Vec::new(),
        images: Vec::new(),
        meta: Some(json!({
            "teamRunStatus": "completed",
            "team": { "name": "team-demo" },
            "agentFinalResponses": [
                {
                    "agent": "builder",
                    "status": "finished",
                    "lastResponse": "Built the feature."
                },
                {
                    "agent": "reviewer",
                    "status": "finished",
                    "lastResponse": "Reviewed the result."
                }
            ]
        })),
    };

    let completion = agent_swarm_completion_from_event(&event)
        .expect("completed TeamRun event should trigger a swarm completion wake");

    assert_eq!(completion.team_name, "team-demo");
    assert_eq!(completion.responses.len(), 2);
    assert_eq!(completion.responses[0].agent, "builder");
    assert_eq!(completion.responses[0].last_response, "Built the feature.");
}

#[test]
fn swarm_completion_wake_text_mentions_finished_and_agent_responses() {
    let completion = AgentSwarmCompletion {
        team_name: "team-demo".to_string(),
        responses: vec![AgentSwarmFinalResponse {
            agent: "builder".to_string(),
            status: "finished".to_string(),
            last_response: "Built the feature.".to_string(),
            last_error: None,
        }],
    };

    let wake_text = agent_swarm_completion_wake_text(&completion);

    assert!(wake_text.contains("<agent_swarm_finished>"));
    assert!(wake_text.contains("agent: @builder"));
    assert!(wake_text.contains("Built the feature."));
    assert!(wake_text.contains("Agent Swarm a terminé"));
}

#[test]
fn the_plan_file_gets_the_plan_not_the_closing_line() {
    let text = |role: Role, value: &str, meta: Option<Value>| ChatMessage {
        role,
        parts: vec![Part::Text {
            text: value.into(),
            meta,
        }],
    };
    let plan = "# Plan\n\n1. Add the table\n2. Wire the API\n3. Test it end to end";
    let history = vec![
        text(Role::User, "plan the feature", None),
        text(Role::Assistant, plan, None),
        text(Role::Assistant, "Here is the plan, tell me.", None),
    ];
    assert_eq!(plan_message_index(&history, 0), Some(1));

    // A message marked as the plan wins even when shorter.
    let mut marked = history.clone();
    marked.push(text(Role::Assistant, "Short plan.", Some(json!({ "plan_proposal": true }))));
    marked.push(text(Role::Assistant, "Done, waiting for you.", None));
    assert_eq!(plan_message_index(&marked, 0), Some(3));

    // Earlier turns are not considered.
    assert_eq!(plan_message_index(&history, 2), None);
}

#[test]
fn plan_steps_become_the_todo_list_and_get_checked_off() {
    let plan = "# Plan\n\n## Goal\nShip it.\n\n## Steps\n- [ ] Add the table — done when the migration runs\n- [x] Read the docs\n- [ ] Wire the API — done when /api/x answers\n\n## Implementation notes\n- [ ] not a step\n";
    let todo = todo_from_plan(plan).unwrap();
    let texts: Vec<_> = todo.tasks.iter().map(|task| task.text.as_str()).collect();
    assert_eq!(
        texts,
        ["Add the table — done when the migration runs", "Read the docs", "Wire the API — done when /api/x answers"]
    );
    assert!(matches!(todo.tasks[1].status, TodoStatus::Done));
    assert!(todo_from_plan("# No steps here\n").is_none());

    let mut progress = todo.clone();
    progress.tasks[0].status = TodoStatus::Done;
    let checked = plan_with_checked_steps(plan, &progress);
    assert!(checked.contains("- [x] Add the table"));
    assert!(checked.contains("- [ ] Wire the API"));
    assert!(checked.contains("- [ ] not a step"));
    assert!(checked.ends_with('\n'));
}

#[test]
fn a_written_plan_wins_and_question_turns_are_not_plans() {
    let assistant = |parts: Vec<Part>| ChatMessage { role: Role::Assistant, parts };
    let say = |value: &str| Part::Text { text: value.into(), meta: None };
    let call = |name: &str, input: Value| Part::ToolCall { id: "t".into(), name: name.into(), input, meta: None };
    let user = ChatMessage { role: Role::User, parts: vec![say("go")] };

    let history = vec![
        user.clone(),
        assistant(vec![say("A long message that is not the plan at all, just thinking out loud."), call("write_plan", json!({ "markdown": "# Real plan" }))]),
        assistant(vec![say("Saved.")]),
    ];
    assert_eq!(written_plan(&history, 0), Some((1, "# Real plan".to_string())));

    let questions = vec![user, assistant(vec![say("Which one?"), call("question", json!({}))])];
    assert!(turn_has_question_tool(&questions, 0));
}
