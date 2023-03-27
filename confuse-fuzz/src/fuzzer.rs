use std::{
    ffi::OsStr,
    io::{stdout, BufRead, BufReader},
    path::PathBuf,
    process::{Child, Stdio},
    thread::{spawn, JoinHandle},
};

use anyhow::{bail, Result};
use confuse_module::{
    interface::BOOTSTRAP_SOCKNAME as CONFUSE_MODULE_BOOTSTRAP_SOCKNAME,
    interface::CRATE_NAME as CONFUSE_MODULE_CRATE_NAME,
    interface::LOGLEVEL_VARNAME as CONFUSE_MODULE_LOGLEVEL_VARNAME,
    messages::{FuzzerEvent, InitInfo, SimicsEvent, StopType},
};
use confuse_simics_module::find_module;
use confuse_simics_project::SimicsProject;
use crossterm::{
    cursor::Show,
    event::DisableMouseCapture,
    execute,
    terminal::{disable_raw_mode, Clear, ClearType, LeaveAlternateScreen},
};
use ipc_channel::ipc::{IpcOneShotServer, IpcReceiver, IpcSender};
use ipc_shm::{IpcShm, IpcShmWriter};
use libafl::{
    prelude::{tui::TuiMonitor, *},
    Fuzzer as _,
};
use log::{debug, error, info, warn, Level};

/// Customizable fuzzer for SIMICS
pub struct Fuzzer {
    simics_project: SimicsProject,
    tx: IpcSender<FuzzerEvent>,
    rx: IpcReceiver<SimicsEvent>,
    simics: Child,
    _shm: IpcShm,
    shm_writer: IpcShmWriter,
    output_reader: JoinHandle<()>,
    err_reader: JoinHandle<()>,
    input_corpus: PathBuf,
}

impl Fuzzer {
    pub fn try_new<S: AsRef<OsStr>>(
        input_corpus: PathBuf,
        init_info: InitInfo,
        app_yml_path: S,
        simics_project: SimicsProject,
        simics_log_level: Level,
    ) -> Result<Self> {
        let confuse_module = find_module(CONFUSE_MODULE_CRATE_NAME)?;
        let simics_project =
            simics_project.try_with_module(CONFUSE_MODULE_CRATE_NAME, confuse_module)?;
        let (bootstrap, bootstrap_name) = IpcOneShotServer::new()?;
        let mut simics_command = simics_project.command();
        let simics_command = simics_command
            .args(simics_project.module_load_args())
            .arg(app_yml_path)
            .arg("-batch-mode")
            .arg("-e")
            .arg("@SIM_main_loop()")
            .current_dir(&simics_project.base_path)
            .env(CONFUSE_MODULE_BOOTSTRAP_SOCKNAME, bootstrap_name)
            .env(CONFUSE_MODULE_LOGLEVEL_VARNAME, simics_log_level.as_str())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut simics = simics_command.spawn()?;

        let stdout = simics.stdout.take().expect("Could not get stdout");
        let stderr = simics.stderr.take().expect("Could not get stderr");

        let output_reader = spawn(move || {
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();
            loop {
                line.clear();
                let rv = reader.read_line(&mut line).expect("Could not read line");
                if rv == 0 {
                    break;
                }
                let logline = line.trim();
                if !logline.is_empty() {
                    info!("{}", line.trim());
                }
            }
            info!("Output reader exited.");
        });

        let err_reader = spawn(move || {
            let mut reader = BufReader::new(stderr);
            let mut line = String::new();
            loop {
                line.clear();
                let rv = reader.read_line(&mut line).expect("Could not read line");
                if rv == 0 {
                    break;
                }
                let logline = line.trim();
                if !logline.is_empty() {
                    debug!("{}", line.trim());
                }
            }
            info!("Err reader exited.");
        });

        let (_, (tx, rx)): (_, (IpcSender<FuzzerEvent>, IpcReceiver<SimicsEvent>)) =
            bootstrap.accept()?;

        tx.send(FuzzerEvent::Initialize(init_info))?;

        info!("Receiving ipc shm");

        let mut shm = match rx.recv()? {
            SimicsEvent::SharedMem(shm) => shm,
            _ => bail!("Unexpected message received"),
        };

        let shm_writer = shm.writer()?;

        info!("Got writer");

        info!("Sending initial reset signal");

        tx.send(FuzzerEvent::Reset)?;

        Ok(Self {
            simics_project,
            tx,
            rx,
            simics,
            _shm: shm,
            shm_writer,
            output_reader,
            err_reader,
            input_corpus,
        })
    }

    fn run_inner(&mut self, cycles: Option<u64>) -> Result<()> {
        let coverage_observer = unsafe {
            HitcountsMapObserver::new(StdMapObserver::from_mut_ptr(
                "map",
                self.shm_writer.as_mut_ptr(),
                self.shm_writer.len(),
            ))
        };

        let mut coverage_feedback = MaxMapFeedback::new(&coverage_observer);

        let mut objective = CrashFeedback::new();

        let input_corpus = InMemoryCorpus::new();

        let mut state = StdState::new(
            StdRand::with_seed(current_nanos()),
            input_corpus,
            OnDiskCorpus::new(self.simics_project.base_path.join("crashes"))?,
            &mut coverage_feedback,
            &mut objective,
        )?;

        let mon = TuiMonitor::new("Confuse Fuzzer".to_string(), true);
        let mut mgr = SimpleEventManager::new(mon);
        let scheduler = QueueScheduler::new();
        let mut fuzzer = StdFuzzer::new(scheduler, coverage_feedback, objective);
        let mut harness = |input: &BytesInput| {
            let target = input.target_bytes();
            let buf = target.as_slice();
            let run_input = buf.to_vec();
            let mut exit_kind = ExitKind::Ok;
            // We expect we'll get a simics ready message:

            info!("Running with input '{:?}'", run_input);
            match self.rx.recv().expect("Failed to receive message") {
                SimicsEvent::Ready => {
                    debug!("Received ready signal");
                }
                _ => {
                    error!("Received unexpected event");
                }
            }

            info!("Sending run signal");
            self.tx
                .send(FuzzerEvent::Run(run_input))
                .expect("Failed to send message");

            match self.rx.recv().expect("Failed to receive message") {
                SimicsEvent::Stopped(stop_type) => match stop_type {
                    StopType::Crash(fault) => {
                        error!("Target crashed with fault {:?}, yeehaw!", fault);
                        exit_kind = ExitKind::Crash;
                    }
                    StopType::Normal => {
                        info!("Target stopped normally ;_;");

                        exit_kind = ExitKind::Ok;
                    }
                    StopType::TimeOut => {
                        warn!("Target timed out, yeehaw(???)");
                        exit_kind = ExitKind::Timeout;
                    }
                },
                _ => {
                    error!("Received unexpected event");
                }
            }

            // We'd read the state of the vm here, including caught exceptions and branch trace
            // Now we send the reset signal
            debug!("Sending reset signal");

            self.tx
                .send(FuzzerEvent::Reset)
                .expect("Failed to send message");

            debug!("Harness done");

            exit_kind
        };

        info!("Creating executor");

        let mut executor = InProcessExecutor::new(
            &mut harness,
            tuple_list!(coverage_observer),
            &mut fuzzer,
            &mut state,
            &mut mgr,
        )?;

        if state.corpus().count() < 1 {
            state.load_initial_inputs(
                &mut fuzzer,
                &mut executor,
                &mut mgr,
                &[self.input_corpus.clone()],
            )?;
            info!("Loaded {} initial inputs", state.corpus().count());
        }

        info!("Creating mutator");

        let mutator = StdScheduledMutator::new(havoc_mutations());

        let mut stages = tuple_list!(StdMutationalStage::new(mutator));

        info!("Starting fuzz loop");

        match cycles {
            Some(cycles) => {
                fuzzer.fuzz_loop_for(&mut stages, &mut executor, &mut state, &mut mgr, cycles)?;
            }
            None => {
                fuzzer.fuzz_loop(&mut stages, &mut executor, &mut state, &mut mgr)?;
            }
        }

        Ok(())
    }

    pub fn run(&mut self) -> Result<()> {
        self.run_inner(None)
    }

    pub fn run_cycles(&mut self, cycles: u64) -> Result<()> {
        self.run_inner(Some(cycles))
    }

    pub fn stop(mut self) -> Result<()> {
        info!("Stopping the fuzzer.");
        // We expect we'll get a simics ready message:
        // TODO: Do we need to figure out how to exit cleanly on the simics side or can we just
        // kill?

        // match self.rx.recv()? {
        //     SimicsEvent::Ready => {
        //         info!("Received ready signal");
        //     }
        //     _ => {
        //         error!("Received unexpected event");
        //     }
        // }

        // self.tx.send(FuzzerEvent::Stop)?;

        self.simics.kill()?;

        // At this point, we don't care if this succeeds or not.
        self.output_reader.join().ok();
        self.err_reader.join().ok();

        info!("Stopped. Bye!");

        // TODO: PR a fix for this to libafl to make this not necessary
        // The TUI Monitor doesn't clean itself up nicely so we do this for now
        disable_raw_mode()?;
        execute!(
            stdout(),
            LeaveAlternateScreen,
            DisableMouseCapture,
            Show,
            Clear(ClearType::Purge)
        )?;

        info!("Stopped fuzzer.");

        Ok(())
    }
}
