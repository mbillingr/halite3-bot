use behavior_tree::{interrupt, lambda, run_or_fail, select, sequence, BtNode, BtState};
use hlt::direction::Direction;
use hlt::ShipId;
use GameState;
use rand::{thread_rng, Rng};

fn deliver(id: ShipId) -> Box<impl BtNode<GameState>> {
    let mut turns_taken = 0;
    lambda(move |state: &mut GameState| {
        if state.get_ship(id).halite <= 0 {
            state.notify_return(turns_taken);
            return BtState::Success;
        }

        let pos = state.get_ship(id).position;
        //let dest = state.me().shipyard.position;
        //let path = state.get_dijkstra_path(pos, dest);
        //let d = path.first().cloned().unwrap_or(Direction::Still);
        let d = state.get_return_dir(pos);
        if !state.try_move_ship(id, d) {
            let d = state.get_return_dir_alternative(pos);
            state.move_ship_or_wait(id, d);
        }

        turns_taken += 1;

        BtState::Running
    })
}

fn go_home(id: ShipId) -> Box<impl BtNode<GameState>> {
    lambda(move |state: &mut GameState| {
        let pos = state.get_ship(id).position;
        //let dest = state.me().shipyard.position;
        //let path = state.get_dijkstra_path(pos, dest);
        //let d = path.first().cloned().unwrap_or(Direction::Still);
        let d = state.get_return_dir(pos);
        let p = pos.directional_offset(d);

        if state.game.map.at_position(&p).structure.is_some() {
            state.move_ship(id, d);
        } else {
            if !state.try_move_ship(id, d) {
                let d = state.get_return_dir_alternative(pos);
                state.move_ship_or_wait(id, d);
            }
        }

        BtState::Running
    })
}

/*fn go_to(id: ShipId, dest: Position) -> Box<impl BtNode<GameState>> {
    lambda(move |state: &mut GameState| {
        if state.get_ship(id).position == dest {
            return BtState::Success;
        }

        let pos = state.get_ship(id).position;
        let path = state.get_dijkstra_path(pos, dest);
        let d = path.first().cloned().unwrap_or(Direction::Still);
        state.move_ship_or_wait(id, d);

        BtState::Running
    })
}*/

fn find_res(id: ShipId) -> Box<impl BtNode<GameState>> {
    lambda(move |state: &mut GameState| {
        const SEEK_LIMIT: usize = 50;

        let pos = state.get_ship(id).position;
        let current_halite = state.game.map.at_position(&pos).halite;

        if current_halite >= SEEK_LIMIT {
            return BtState::Success;
        }

        let d = Direction::get_all_options().into_iter()
            .map(|d| (d, state.game.map.normalize(&pos.directional_offset(d))))
            .filter(|(_, p)| state.navi.is_safe(p) || *p == pos)
            .max_by_key(|(_, p)| (state.get_pheromone(*p) * 1000.0) as i32)
            .map(|(d, _)| d)
            .unwrap_or(Direction::Still);

        state.move_ship(id, d);

        BtState::Running

        /*match state.get_nearest_halite_move(pos, SEEK_LIMIT) {
            Some(d) => {
                state.move_ship(id, d);
                BtState::Running
            }
            None => BtState::Failure,
        }*/
    })
}

fn find_desperate(id: ShipId) -> Box<impl BtNode<GameState>> {
    lambda(move |state: &mut GameState| {
        let pos = state.get_ship(id).position;
        let current_halite = state.game.map.at_position(&pos).halite;

        if current_halite > 0 {
            return BtState::Success;
        }

        match state.get_nearest_halite_move(pos, 1) {
            Some(d) => {
                state.move_ship(id, d);
                BtState::Running
            }
            None => BtState::Failure,
        }
    })
}

fn greedy(id: ShipId) -> Box<impl BtNode<GameState>> {
    lambda(move |state: &mut GameState| {
        const PREFER_STAY_FACTOR: usize = 2;
        const HARVEST_LIMIT: usize = 10;
        const SEEK_LIMIT: usize = 50;
        const PHEROMONE_WEIGHT: f64 = 1.0;

        if state.get_ship(id).is_full() {
            return BtState::Success;
        }

        let (pos, cargo) = {
            let ship = state.get_ship(id);
            (ship.position, ship.halite)
        };

        let movement_cost =
            state.game.map.at_position(&pos).halite / state.game.constants.move_cost_ratio;

        if cargo < movement_cost {
            state.move_ship(id, Direction::Still);
            return BtState::Running;
        }

        let syp = state.me().shipyard.position;

        let current_halite = state.game.map.at_position(&pos).halite;
        let current_value = current_halite / state.game.constants.extract_ratio;

        if current_halite >= SEEK_LIMIT {
            state.move_ship(id, Direction::Still);
            return BtState::Running;
        }

        let mov = Direction::get_all_cardinals()
            .into_iter()
            .map(|d| (d, pos.directional_offset(d)))
            .map(|(d, p)| {
                (
                    state.game.map.at_position(&p).halite,
                    state.game.map.at_position(&p).halite / state.game.constants.extract_ratio,
                    d,
                    p,
                )
            })
            .filter(|&(halite, _, _, _)| halite >= HARVEST_LIMIT)
            .filter(|&(_, _, _, p)| p != syp)
            .filter(|&(_, value, _, _)| value > movement_cost + current_value * PREFER_STAY_FACTOR)
            .filter(|(_, _, _, p)| state.navi.is_safe(p))
            .map(|(halite, value, d, p)| (halite, value + (state.get_pheromone(p) * PHEROMONE_WEIGHT) as usize, d, p))
            .max_by_key(|&(_, value, _, _)| value)
            .map(|(_, _, d, p)| (d, p));

        if mov.is_none() && current_halite < SEEK_LIMIT {
            return BtState::Failure;
        }

        let (d, _) = mov.unwrap_or((Direction::Still, pos));

        state.move_ship(id, d);

        BtState::Running
    })
}

fn desperate(id: ShipId) -> Box<impl BtNode<GameState>> {
    lambda(move |state: &mut GameState| {
        if state.get_ship(id).is_full() {
            return BtState::Success;
        }

        let pos = state.get_ship(id).position;

        let movement_cost =
            state.game.map.at_position(&pos).halite / state.game.constants.move_cost_ratio;

        if movement_cost > 0 {
            state.move_ship(id, Direction::Still);
            return BtState::Running;
        }

        let syp = state.me().shipyard.position;

        let mov = Direction::get_all_options()
            .into_iter()
            .map(|d| (d, pos.directional_offset(d)))
            .map(|(d, p)| (state.game.map.at_position(&p).halite, d, p))
            .filter(|&(halite, _, _)| halite > 0)
            .filter(|&(_, _, p)| p != syp)
            .filter(|(_, _, p)| state.navi.is_safe(p))
            .max_by_key(|&(halite, _, _)| halite)
            .map(|(_, d, _)| d);

        match mov {
            None => BtState::Failure,
            Some(d) => {
                state.move_ship(id, d);
                BtState::Running
            }
        }
    })
}

pub fn build_dropoff(id: ShipId) -> Box<impl BtNode<GameState>> {
    run_or_fail(move |state: &mut GameState| state.try_build_dropoff(id))
}

pub fn collector(id: ShipId) -> Box<impl BtNode<GameState>> {
    select(vec![
        interrupt(
            select(vec![
                sequence(vec![greedy(id), deliver(id)]),
                find_res(id),
                sequence(vec![desperate(id), deliver(id)]),
                find_desperate(id),
            ]),
            move |env| {
                const GO_HOME_SAFETY_FACTOR: usize = 1;

                let dist = env.get_return_distance(env.get_ship(id).position);
                env.rounds_left() <= dist + (env.me().ship_ids.len() * GO_HOME_SAFETY_FACTOR) / (1 + env.me().dropoff_ids.len())
            },
        ),
        go_home(id),
    ])
}

pub fn kamikaze(id: ShipId) -> Box<impl BtNode<GameState>> {
    go_home(id)
}
