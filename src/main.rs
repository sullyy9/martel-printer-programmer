use std::{path::Path, sync::Arc, time::Duration};

use probe_rs::{
    architecture::arm::DpAddress,
    config::{add_target_from_yaml, get_target_by_name, MemoryRegion, TargetSelector},
    flashing::{
        download_file, download_file_with_options, erase_all, DownloadOptions, FlashProgress,
        Format, ProgressEvent,
    },
    DebugProbeError, MemoryInterface, Permissions, Probe, Target, WireProtocol,
};

const STM32F1: &[u8] = include_bytes!("../res/STM32F1xx.yaml");
const STM32F2: &[u8] = include_bytes!("../res/STM32F2xx.yaml");
const STM32L4: &[u8] = include_bytes!("../res/STM32L4xx.yaml");

macro_rules! extract_resource {
    ($from:literal => $to:literal) => {
        std::fs::write(
            concat!("./", $to),
            include_bytes!(concat!("../res/", $from)),
        )
        .expect(concat!("Failed to extract resource: ", $from));
    };
}

fn main() -> Result<(), probe_rs::Error> {
    let probe_list = Probe::list_all();

    println!("Probes:");
    probe_list
        .iter()
        .for_each(|probe| println!("Probe found => {}", probe.identifier));
    println!("--------------------");

    let mut probe = Probe::open(&probe_list[0])?;

    //probe.select_protocol(WireProtocol::Jtag)?;
    probe.attach_to_unspecified()?;

    let target_id = probe
        .try_into_arm_interface()
        .map_err(|(_, err)| probe_rs::Error::from(err))
        .and_then(|mut interface| interface.read_dpidr().map_err(probe_rs::Error::from))?;

    let target_name = match target_id {
        0x2ba01477 => {
            extract_resource!("STM32L4xx.yaml" => "target");
            add_target_from_yaml(Path::new("./target"))?;
            "STM32L433RCTx".to_string()
        }
        0x1ba01477 => "STM32F103RC".to_string(),
        id => format!("Unknown({})", id),
    };

    println!("Found target: {}", target_name);

    let mut session = Probe::open(&probe_list[0])
        .map_err(probe_rs::Error::from)
        .and_then(|probe| probe.attach(target_name, Permissions::new().allow_erase_all()))?;

    println!("pog: {:#?}", session.target());

    let mut ram = Vec::new();
    let mut flash = Vec::new();
    let mut generic = Vec::new();
    session
        .target()
        .memory_map
        .iter()
        .for_each(|region| match region.clone() {
            MemoryRegion::Generic(gen_region) => generic.push(gen_region),
            MemoryRegion::Ram(ram_region) => ram.push(ram_region),
            MemoryRegion::Nvm(flash_region) => flash.push(flash_region),
        });

    println!();
    println!("Memory regions");
    ram.iter().for_each(|region| {
        println!(
            "Found RAM Region => {} : {:#x?}",
            region.name.as_ref().unwrap_or(&"unnamed".to_string()),
            region.range
        );
    });
    flash.iter().for_each(|region| {
        println!(
            "Found Flash Region => {} : {:#x?}",
            region.name.as_ref().unwrap_or(&"unnamed".to_string()),
            region.range
        );
    });
    generic.iter().for_each(|region| {
        println!(
            "Found Generic Region => {} : {:#x?}",
            region.name.as_ref().unwrap_or(&"unnamed".to_string()),
            region.range
        );
    });

    let flash = flash[0].clone();
    let ram = ram[0].clone();

    println!("cores: {:?}", session.list_cores());

    let core_halted = if let Ok(mut core) = session.core(0) {
        core.reset_and_halt(Duration::from_secs(1))?;
        core.core_halted()?
    } else {
        false
    };

    if core_halted {
        extract_resource!("../res/stm.hex" => "./firmware");

        let mut options = DownloadOptions::default();
        let progress = FlashProgress::new(flash_progress_handler);
        options.progress = Some(&progress);
        options.do_chip_erase = true;
        options.skip_erase = false;
        options.verify = true;


        download_file_with_options(
            &mut session,
            "./firmware",
            Format::Hex,
            options
        );
    } else {
        println!("ERROR => Failed to halt core");
    }

    Ok(())
}

fn flash_progress_handler(event: ProgressEvent) {
    use ProgressEvent::*;
    match event {
        Initialized { flash_layout } => println!("---Program Begin---"),

        StartedFilling => println!("Fill start"),
        PageFilled { size, time } => (),
        FailedFilling => println!("Fill fail"),
        FinishedFilling => println!("Fill complete"),

        StartedErasing => println!("Erase start"),
        SectorErased { size, time } => (),
        FailedErasing => println!("Erase fail"),
        FinishedErasing => println!("Erase complete"),

        StartedProgramming => println!("Program start"),
        PageProgrammed { size, time } => (),
        FailedProgramming => println!("Program fail"),
        FinishedProgramming => println!("Program complete"),
    }
}
