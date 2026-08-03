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

    assert_eq!(
        unsafe { pg.call_unchecked(&identity_function, &[256i64.into()])? },
        256i64.into()
    );

    let dispatch_function = pg
        .function("dispatch")
        .expect("Wasnt able to find `dispatch` function");

    for i in 0..=16 {
        // prints numbers 0..=16 using purple gardens io.println
        pg.call::<_, ()>(&dispatch_function, i)?;
    }

    Ok(())
}
