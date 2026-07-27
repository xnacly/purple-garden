use std::fs;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let filename = concat!(env!("CARGO_MANIFEST_DIR"), "/program.garden");
    let program = fs::read(filename).expect("Failed to find dispatch.garden");
    let mut pg = match purple_garden::Pg::new()
        .with_unsafe_stdlib()
        .with_stdlib()
        .compile(&program)
    {
        Ok(pg) => pg,
        Err(err) => {
            eprintln!("{}", &err.render(filename, &program));
            return Err(err.into());
        }
    };

    let identity_function = pg
        .function("identity")
        .expect("Wasnt able to find `identity` function");

    assert_eq!(pg.call::<i64, i64>(identity_function, &[256])?, 256);

    let dispatch_function = pg
        .function("dispatch")
        .expect("Wasnt able to find `dispatch` function");

    for i in 0..=16 {
        // prints numbers 0..=16 using purple gardens io.println
        let _: () = pg.call(dispatch_function, &[i])?;
    }

    Ok(())
}
