/*use std::collections::HashMap;

use coin_cbc::raw::Status;
use good_lp::{coin_cbc, Constraint, ResolutionError, SolverModel, Variable, WithMipGap};
use good_lp::constraint::ConstraintReference;
use good_lp::solvers::coin_cbc::CoinCbcProblem;
use good_lp::solvers::MipGapError;
use good_lp::variable::UnsolvedProblem;

struct CoinCbcEarlyTerminationProblem {
    solver: CoinCbcProblem,
    mip_gap: Option<f32>
}

fn coin_cbc_with_early_termination(to_solve: UnsolvedProblem) -> CoinCbcEarlyTerminationProblem {
    let solver = coin_cbc(to_solve);
    CoinCbcEarlyTerminationProblem { solver, mip_gap: None }
}

impl CoinCbcEarlyTerminationProblem {
    pub fn inner(&self) -> &CoinCbcProblem {
        &self.solver
    }
}
impl WithMipGap for CoinCbcEarlyTerminationProblem {
    fn mip_gap(&self) -> Option<f32> {
        self.mip_gap
    }
    fn with_mip_gap(mut self, mip_gap: f32) -> Result<Self, MipGapError> {
        if mip_gap.is_sign_negative() {
            Err(MipGapError::Negative)
        } else if mip_gap.is_infinite() {
            Err(MipGapError::Infinite)
        } else {
            self.mip_gap = Some(mip_gap);
            Ok(self)
        }
    }
}

struct EarlyTermCoinCbcSolution {
    pub inner: coin_cbc::Solution,
    solution_vec: Vec<f64>
}

impl SolverModel for CoinCbcEarlyTerminationProblem {
    type Solution = HashMap<Variable, f64>;
    type Error = ResolutionError;
    
    fn solve(mut self) -> Result<Self::Solution, Self::Error> {
        let solver = &mut self.solver;
        if let Some(mip_gap) = self.mip_gap {
            solver.set_parameter("ratiogap", &mip_gap.to_string());
        }

        let solution = solver.as_inner().solve();
        let raw = solution.raw();
        match raw.status() {
            // Status::Stopped => Err(ResolutionError::Other("Stopped")),
            Status::Abandoned => Err(ResolutionError::Other("Abandoned")),
            Status::UserEvent => Err(ResolutionError::Other("UserEvent")),
            Status::Finished 
            | Status::Unlaunched 
            | Status::Stopped => {
                if raw.is_continuous_unbounded() {
                    Err(ResolutionError::Unbounded)
                } else if raw.is_proven_infeasible() {
                    Err(ResolutionError::Infeasible)
                } else {
                    let raw = solution.raw();
                    let solution_vec = raw.col_solution()
                        .iter()
                        .enumerate()
                        .map(|(i, &val)| (solver.variables[i], val))
                    
                }
            },
        }
    }

    fn add_constraint(&mut self, c: Constraint) -> ConstraintReference {
        self.solver.add_constraint(c)
    }

    fn name() -> &'static str {
        "CoinCBC with early termination"
    }


}*/