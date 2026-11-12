//! One folder per agent runtime. Domain code talks to [`crate::harness::Harness`], not to these types.
pub(crate) mod t3;

#[cfg(test)]
pub(crate) mod mock;
