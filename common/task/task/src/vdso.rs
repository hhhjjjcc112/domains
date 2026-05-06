use basic::{AlienResult, vdso_map_user};
use crate::processor::current_task;

pub(crate) fn load_vdso() -> AlienResult<usize> {
    let task = current_task().unwrap();
    let user_vdso_base = vdso_map_user(task.token())?;

    Ok(user_vdso_base)
}