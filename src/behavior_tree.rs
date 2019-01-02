use std::marker::PhantomData;

type NodePtr<E> = Box<dyn BtNode<E>>;

pub trait BtNode<E> {
    fn tick(&mut self, env: &mut E) -> BtState;
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum BtState {
    NotStarted,
    Running,
    Success,
    Failure,
}

pub fn lambda<E, F: FnMut(&mut E) -> BtState>(mut func: F) -> Box<impl BtNode<E>> {
    Box::new(Lambda::new(func))
}

pub fn condition<E, P: FnMut(&mut E) -> bool>(mut func: P) -> Box<impl BtNode<E>> {
    lambda(move |e| {
        if func(e) {
            BtState::Success
        } else {
            BtState::Failure
        }
    })
}

pub fn sequence<E>(children: Vec<NodePtr<E>>) -> Box<impl BtNode<E>> {
    Box::new(Sequence::new(children))
}

pub fn select<E>(children: Vec<NodePtr<E>>) -> Box<impl BtNode<E>> {
    Box::new(Selector::new(children))
}

pub fn run_or_fail<E, P: FnMut(&mut E) -> bool>(mut func: P) -> Box<impl BtNode<E>> {
    let mut state = BtState::NotStarted;
    lambda(move |e| match state {
        BtState::Running => {
            state = BtState::NotStarted;
            BtState::Success
        }
        BtState::NotStarted => {
            if func(e) {
                state = BtState::Running;
                BtState::Running
            } else {
                BtState::Failure
            }
        }
        BtState::Failure | BtState::Success => unreachable!(),
    })
}

struct Lambda<F, E>
where
    F: FnMut(&mut E) -> BtState,
{
    func: F,
    _e: PhantomData<E>,
}

impl<F, E> Lambda<F, E>
where
    F: FnMut(&mut E) -> BtState,
{
    fn new(func: F) -> Lambda<F, E> {
        Lambda {
            func,
            _e: PhantomData,
        }
    }
}

impl<F, E> BtNode<E> for Lambda<F, E>
where
    F: FnMut(&mut E) -> BtState,
{
    fn tick(&mut self, env: &mut E) -> BtState {
        (self.func)(env)
    }
}

struct Sequence<E> {
    children: Vec<NodePtr<E>>,
    current_child: usize,
}

impl<E> Sequence<E> {
    fn new(children: Vec<NodePtr<E>>) -> Sequence<E> {
        Sequence {
            children,
            current_child: 0,
        }
    }
}

impl<E> BtNode<E> for Sequence<E> {
    fn tick(&mut self, env: &mut E) -> BtState {
        loop {
            let ret = self.children[self.current_child].tick(env);

            match ret {
                BtState::Running => return BtState::Running,
                BtState::Failure => {
                    self.current_child = 0;
                    return BtState::Failure;
                }
                BtState::Success => {
                    self.current_child += 1;
                    if self.current_child == self.children.len() {
                        self.current_child = 0;
                        return BtState::Success;
                    }
                }
                BtState::NotStarted => panic!("Child tick returned NotStarted"),
            }
        }
    }
}

struct Selector<E> {
    children: Vec<NodePtr<E>>,
    current_child: usize,
}

impl<E> Selector<E> {
    fn new(children: Vec<NodePtr<E>>) -> Selector<E> {
        Selector {
            children,
            current_child: 0,
        }
    }
}

impl<E> BtNode<E> for Selector<E> {
    fn tick(&mut self, env: &mut E) -> BtState {
        loop {
            let ret = self.children[self.current_child].tick(env);

            match ret {
                BtState::Running => return BtState::Running,
                BtState::Success => {
                    self.current_child = 0;
                    return BtState::Success;
                }
                BtState::Failure => {
                    self.current_child += 1;
                    if self.current_child == self.children.len() {
                        self.current_child = 0;
                        return BtState::Failure;
                    }
                }
                BtState::NotStarted => panic!("Child tick returned NotStarted"),
            }
        }
    }
}
