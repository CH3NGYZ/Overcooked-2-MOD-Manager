use super::super::filetime_now;
use super::*;
use crate::elevation::ManagerProcess;
use std::process::{Child, Command, Stdio};
use winapi::um::{
    synchapi::{OpenEventW, SetEvent},
    winnt::EVENT_MODIFY_STATE,
};

const NONCE: &str = "OC2_TEST_UPDATE_HANDOFF_NONCE";
const BEHAVIOR: &str = "OC2_TEST_UPDATE_HANDOFF_BEHAVIOR";
const RECEIPT: &str = "OC2_TEST_UPDATE_HANDOFF_RECEIPT";

struct Fixture(Child);

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn spawn_fixture(nonce: uuid::Uuid, behavior: &str) -> Child {
    let owner = ManagerProcess::current().unwrap();
    Command::new(std::env::current_exe().unwrap())
        .args([
            "--ignored",
            "--exact",
            "app_update::windows_session::handoff::tests::pipe_fixture",
        ])
        .env(NONCE, nonce.to_string())
        .env(BEHAVIOR, behavior)
        .env("OC2_TEST_UPDATE_HANDOFF_PID", owner.pid.to_string())
        .env(
            "OC2_TEST_UPDATE_HANDOFF_CREATION",
            owner.created.to_string(),
        )
        .stdout(Stdio::null())
        .spawn()
        .unwrap()
}

fn fixture(nonce: uuid::Uuid, behavior: &str) -> Fixture {
    Fixture(spawn_fixture(nonce, behavior))
}

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(5)
}

fn wait_for_fixture(child: &mut Fixture) {
    let limit = deadline();
    loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            assert!(status.success());
            return;
        }
        assert!(Instant::now() < limit, "fixture did not exit");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn authenticate_fixture(process: &OwnedHandle) -> io::Result<u64> {
    let handle = raw(process);
    if !same_file::is_same_file(std::env::current_exe()?, process_image(handle)?)? {
        return Err(denied("The fixture executable does not match"));
    }
    creation_time(handle)
}

#[test]
fn live_peer_receives_commit_before_old_instance_exits() {
    let nonce = uuid::Uuid::new_v4();
    let not_before = filetime_now().unwrap();
    let mut child = fixture(nonce, "accept");
    negotiate(nonce, deadline(), |process| {
        let created = authenticate_fixture(process)?;
        assert!(created >= not_before);
        Ok(created)
    })
    .unwrap();
    wait_for_fixture(&mut child);
}

#[test]
fn committed_successor_is_released_by_exact_owner_exit() {
    let name = format!(r"Local\OC2-Test-Successor-{}", uuid::Uuid::new_v4());
    let name_wide = wide(OsStr::new(&name));
    let receipt =
        owned_handle(unsafe { CreateEventW(null_mut(), 1, 0, name_wide.as_ptr()) }).unwrap();
    let mut owner = Fixture(
        Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "app_update::windows_session::handoff::tests::owner_fixture",
            ])
            .env(RECEIPT, &name)
            .stdout(Stdio::null())
            .spawn()
            .unwrap(),
    );
    wait_for_fixture(&mut owner);
    wait_for(
        &receipt,
        5_000,
        "successor was not released by the owner's exit",
    )
    .unwrap();
}

fn open_receipt(access: u32) -> OwnedHandle {
    let name = wide(&std::env::var_os(RECEIPT).unwrap());
    owned_handle(unsafe { OpenEventW(access, 0, name.as_ptr()) }).unwrap()
}

#[test]
fn false_image_is_not_committed() {
    let nonce = uuid::Uuid::new_v4();
    let identity = Token::current().unwrap().identity().unwrap();
    let mut child = fixture(nonce, "reject");
    let wrong_exe = std::env::current_exe()
        .unwrap()
        .with_file_name("missing-manager.exe");
    let error = commit_to_child(nonce, &wrong_exe, &identity, 0, deadline()).unwrap_err();
    assert!(error.to_string().contains("authenticate ordinary manager"));
    wait_for_fixture(&mut child);
}

#[test]
fn wrong_creation_time_is_not_committed() {
    let nonce = uuid::Uuid::new_v4();
    let mut child = fixture(nonce, "wrong-ready");
    let error = negotiate(nonce, deadline(), authenticate_fixture).unwrap_err();
    assert!(error.to_string().contains("READY did not identify"));
    wait_for_fixture(&mut child);
}

#[test]
fn old_pipe_server_is_rejected_before_ready() {
    let nonce = uuid::Uuid::new_v4();
    let identity = Token::current().unwrap().identity().unwrap();
    let mut child = fixture(nonce, "reject");
    let error = commit_to_child(
        nonce,
        &std::env::current_exe().unwrap(),
        &identity,
        u64::MAX,
        deadline(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("existed before"));
    wait_for_fixture(&mut child);
}

#[test]
fn incorrect_ack_is_abandoned_without_commit() {
    let nonce = uuid::Uuid::new_v4();
    let mut child = fixture(nonce, "wrong-ack");
    let error = negotiate(nonce, deadline(), authenticate_fixture).unwrap_err();
    assert!(error.to_string().contains("receive handoff ACK"));
    wait_for_fixture(&mut child);
}

#[test]
fn exited_successor_abandons_handoff_before_commit() {
    let nonce = uuid::Uuid::new_v4();
    let mut child = fixture(nonce, "exit-before-ack");
    let error = negotiate(nonce, deadline(), authenticate_fixture).unwrap_err();
    assert!(error.to_string().contains("receive handoff ACK"));
    wait_for_fixture(&mut child);
}

#[test]
fn child_does_not_continue_when_parent_disconnects_after_ack() {
    let nonce = uuid::Uuid::new_v4();
    let mut child = fixture(nonce, "no-commit");
    let pipe = connect_to_child(nonce, deadline()).unwrap();
    let link = Link::new(&pipe, None, deadline());
    let mut ready = [0; 9];
    link.receive(&mut ready).unwrap();
    link.send(&[PREPARE]).unwrap();
    link.expect(ACKNOWLEDGED).unwrap();
    drop(pipe);
    wait_for_fixture(&mut child);
}

#[test]
fn only_one_pipe_instance_can_claim_a_nonce() {
    let nonce = uuid::Uuid::new_v4();
    let identity = Token::current().unwrap().identity().unwrap();
    let _pipe = create_pipe(&identity.sid, nonce).unwrap();
    assert!(create_pipe(&identity.sid, nonce).is_err());
}

#[test]
fn absent_peer_has_a_bounded_deadline() {
    let limit = Instant::now() + Duration::from_millis(50);
    let error = connect_to_child(uuid::Uuid::new_v4(), limit).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
}

#[test]
#[ignore = "spawned by native update handoff tests"]
fn pipe_fixture() {
    let nonce = std::env::var(NONCE).unwrap().parse().unwrap();
    let owner = ManagerProcess::parse(
        std::env::var_os("OC2_TEST_UPDATE_HANDOFF_PID"),
        std::env::var_os("OC2_TEST_UPDATE_HANDOFF_CREATION"),
    )
    .unwrap();
    let process = owner.open(&std::env::current_exe().unwrap()).unwrap();
    let created = ManagerProcess::current().unwrap().created;
    let identity = Token::current().unwrap().identity().unwrap();
    let behavior = std::env::var(BEHAVIOR).unwrap();
    if behavior == "wait-for-owner" {
        let receipt = open_receipt(EVENT_MODIFY_STATE);
        wait_for_commit(nonce, &identity.sid, owner.pid, &process, created).unwrap();
        assert_ne!(unsafe { SetEvent(raw(&receipt)) }, 0);
        return;
    }
    let pipe = create_pipe(&identity.sid, nonce).unwrap();
    let receive = |created| receive_owner_commit(&pipe, owner.pid, &process, created, deadline());
    match behavior.as_str() {
        "accept" => receive(created).unwrap(),
        "reject" => assert!(receive(created).is_err()),
        "wrong-ready" => assert!(receive(created + 1).is_err()),
        "no-commit" => assert!(
            receive(created)
                .unwrap_err()
                .to_string()
                .contains("receive handoff COMMIT")
        ),
        behavior @ ("wrong-ack" | "exit-before-ack") => {
            let link = connect_to_owner(&pipe, owner.pid, &process, deadline()).unwrap();
            link.send(&ready_message(created)).unwrap();
            link.expect(PREPARE).unwrap();
            if behavior == "wrong-ack" {
                link.send(&[0]).unwrap();
                assert!(link.expect(COMMIT).is_err());
            }
        }
        _ => panic!("unknown fixture behavior"),
    }
}

#[test]
#[ignore = "spawned by native update handoff tests"]
fn owner_fixture() {
    let nonce = uuid::Uuid::new_v4();
    let mut child = spawn_fixture(nonce, "wait-for-owner");
    negotiate(nonce, deadline(), authenticate_fixture).unwrap();
    assert!(child.try_wait().unwrap().is_none());
    let receipt = open_receipt(SYNCHRONIZE);
    assert_eq!(
        unsafe { WaitForSingleObject(raw(&receipt), 0) },
        WAIT_TIMEOUT
    );
}
