/// Used by the workspace crates to ensure that the builder crate is used when building the project
/// through an `include!()` macro.

#[cfg(not(____builder))]
// ^ uncomment this line to allow building the project without the builder crate in debug mode
const _: () = {
	// compile_error!("This project is meant to be built using the builder helper crate.\nPlease check the ../builder directory for more information.");
};
