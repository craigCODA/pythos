use pythos_shared::pyth_command_abi::{
    COMMAND_FIELD_KIND, COMMAND_FIELD_OBJECT_ID, COMMAND_FIELD_PROPOSAL_ID, COMMAND_FIELD_TASK_ID,
    COMMAND_FIELD_TEXT_UTF8, COMMAND_KIND_APPROVE_PROPOSAL, COMMAND_KIND_CREATE_NOTE,
    COMMAND_KIND_CREATE_TASK, COMMAND_KIND_INSPECT_OBJECT, COMMAND_KIND_LIST_OBJECTS,
    COMMAND_KIND_LIST_PROPOSALS, COMMAND_KIND_LIST_TASKS, COMMAND_KIND_REBOOT,
    COMMAND_KIND_REVISE_NOTE, COMMAND_KIND_REVIVE_TASK, COMMAND_KIND_SUSPEND_TASK,
    COMMAND_KIND_SYSTEM_STATUS, COMMAND_RESULT_STATUS_DENIED, COMMAND_RESULT_STATUS_OK,
    PYTH_COMMAND_ABI_MAJOR, PYTH_COMMAND_ABI_MINOR, PythCommand, PythCommandResult,
};

#[test]
fn command_abi_codes_and_layouts_are_stable() {
    assert_eq!(PYTH_COMMAND_ABI_MAJOR, 1);
    assert_eq!(PYTH_COMMAND_ABI_MINOR, 0);

    assert_eq!(COMMAND_KIND_LIST_OBJECTS, 1);
    assert_eq!(COMMAND_KIND_INSPECT_OBJECT, 2);
    assert_eq!(COMMAND_KIND_CREATE_NOTE, 3);
    assert_eq!(COMMAND_KIND_REVISE_NOTE, 4);
    assert_eq!(COMMAND_KIND_LIST_TASKS, 5);
    assert_eq!(COMMAND_KIND_CREATE_TASK, 6);
    assert_eq!(COMMAND_KIND_LIST_PROPOSALS, 7);
    assert_eq!(COMMAND_KIND_APPROVE_PROPOSAL, 8);
    assert_eq!(COMMAND_KIND_SUSPEND_TASK, 9);
    assert_eq!(COMMAND_KIND_REVIVE_TASK, 10);
    assert_eq!(COMMAND_KIND_SYSTEM_STATUS, 11);
    assert_eq!(COMMAND_KIND_REBOOT, 12);

    assert_eq!(COMMAND_RESULT_STATUS_OK, 0);
    assert_eq!(COMMAND_RESULT_STATUS_DENIED, 1);

    assert_eq!(COMMAND_FIELD_KIND, 0);
    assert_eq!(COMMAND_FIELD_OBJECT_ID, 1);
    assert_eq!(COMMAND_FIELD_TASK_ID, 2);
    assert_eq!(COMMAND_FIELD_PROPOSAL_ID, 3);
    assert_eq!(COMMAND_FIELD_TEXT_UTF8, 4);

    assert_eq!(core::mem::size_of::<PythCommand>(), 64);
    assert_eq!(core::mem::align_of::<PythCommand>(), 8);
    assert_eq!(core::mem::size_of::<PythCommandResult>(), 48);
    assert_eq!(core::mem::align_of::<PythCommandResult>(), 8);
}
