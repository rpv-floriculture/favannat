use nalgebra::{DMatrix, DVector};

/// Data structures implementing this trait can be used as input and output of networks.
pub trait NetworkIO {
    fn input(input: Self) -> DMatrix<f32>;
    fn output(output: DMatrix<f32>) -> Self;
}

impl NetworkIO for DMatrix<f32> {
    fn input(input: Self) -> DMatrix<f32> {
        input
    }
    fn output(output: DMatrix<f32>) -> Self {
        output
    }
}

impl NetworkIO for DVector<f32> {
    fn input(input: Self) -> DMatrix<f32> {
        DMatrix::from_iterator(1, input.len(), input.into_iter().cloned())
    }
    fn output(output: DMatrix<f32>) -> Self {
        DVector::from(output.into_iter().cloned().collect::<Vec<f32>>())
    }
}

impl NetworkIO for Vec<f32> {
    fn input(input: Self) -> DMatrix<f32> {
        DMatrix::from_iterator(1, input.len(), input.into_iter())
    }
    fn output(output: DMatrix<f32>) -> Self {
        output.into_iter().cloned().collect::<Vec<f32>>()
    }
}

#[cfg(feature = "ndarray")]
use ndarray::Array1;

#[cfg(feature = "ndarray")]
impl NetworkIO for Array1<f32> {
    fn input(input: Self) -> DMatrix<f32> {
        DMatrix::from_iterator(1, input.len(), input.into_iter)
    }
    fn output(output: DMatrix<f32>) -> Self {
        Array1::from_iter(output.into_iter().cloned())
    }
}
