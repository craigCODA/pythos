//! Hardware-neutral input values shared by input producers and session policy.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KeyCode {
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    Digit0,
    Digit1,
    Digit2,
    Digit3,
    Digit4,
    Digit5,
    Digit6,
    Digit7,
    Digit8,
    Digit9,
    Enter,
    Escape,
    Space,
    Backspace,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputSource {
    Keyboard,
    Mouse,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RelativeMotion {
    pub dx: i8,
    pub dy: i8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputEventKind {
    KeyDown(KeyCode),
    RelativeMotion(RelativeMotion),
    PointerButton { left: bool },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InputEvent {
    pub source: InputSource,
    pub kind: InputEventKind,
}
