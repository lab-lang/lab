//! Hardware bring-up for a physically connected Absorbance 96.
//!
//! CI has no hardware, so the test is `#[ignore]`d and additionally gated
//! on an environment variable. With a reader plugged in:
//!
//! ```sh
//! LAB_BYONOY_HARDWARE=1 cargo test -p lab-byonoy --test hardware -- --ignored --nocapture
//! ```

#![cfg(feature = "hid")]

#[test]
#[ignore = "needs a physically connected Absorbance 96; set LAB_BYONOY_HARDWARE=1 and pass --ignored"]
fn bring_up_checklist() {
    if std::env::var_os("LAB_BYONOY_HARDWARE").is_none() {
        eprintln!("LAB_BYONOY_HARDWARE is unset; not touching the bench");
        return;
    }

    // 1. Open: enumerates, opens by path, runs the mandatory 660 nm
    //    reference measurement, and queries the installed wavelengths.
    let mut reader = lab_byonoy::Absorbance96::open()
        .expect("the reader opens and completes its reference measurement");

    // 2. The discrete wavelength list is this unit's LEDs.
    let wavelengths = reader.installed_wavelengths().to_vec();
    println!("installed wavelengths: {wavelengths:?} nm");
    assert!(
        !wavelengths.is_empty(),
        "a unit ships with at least one LED"
    );

    // 3. Slot sensing.
    let status = reader.status().expect("the status query answers");
    println!("slot state: {:?}", status.slot_state);

    // 4. A full-plate read at the first installed wavelength (~65 s).
    let plate = reader
        .measure_absorbance(wavelengths[0])
        .expect("a full-plate read completes");
    println!(
        "A1 at {} nm: {} OD; H12: {} OD",
        plate.wavelength_nm, plate.rows[0][0], plate.rows[7][11]
    );
}
