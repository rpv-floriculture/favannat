use crate::network::{EdgeLike, Fabricator, NetworkLike, NodeLike};
use nalgebra::{DMatrix, DVector};
use std::collections::HashMap;

pub struct MatrixFeedforwardFabricator;

impl MatrixFeedforwardFabricator {
    fn get_matrix(dynamic_matrix: Vec<Vec<f32>>) -> DMatrix<f32> {
        let columns = dynamic_matrix
            .into_iter()
            .map(DVector::from_vec)
            .collect::<Vec<_>>();

        DMatrix::from_columns(&columns)
    }
}

impl<N, E> Fabricator<N, E> for MatrixFeedforwardFabricator
where
    N: NodeLike,
    E: EdgeLike,
{
    type Output = super::evaluator::MatrixFeedforwardEvaluator;

    fn fabricate(net: &impl NetworkLike<N, E>) -> Result<Self::Output, &'static str> {
        // build dependency graph by collecting incoming edges per node
        let mut dependency_graph: HashMap<usize, Vec<&E>> = HashMap::new();

        for edge in net.edges() {
            dependency_graph
                .entry(edge.end())
                .and_modify(|dependencies| dependencies.push(edge))
                .or_insert_with(|| vec![edge]);
        }

        if dependency_graph.is_empty() {
            return Err("no edges present, net invalid");
        }

        // keep track of dependencies present
        let mut dependency_count = dependency_graph.len();

        // println!("initial dependency_graph {:#?}", dependency_graph);

        // contains list of matrices (stages) that form the computable net
        let mut compute_stages: Vec<crate::Matrix> = Vec::new();
        // contains activation functions corresponding to each stage
        let mut stage_transformations: Vec<crate::Transformations> = Vec::new();
        // set available nodes a.k.a net input
        let mut available_nodes: Vec<usize> = net.inputs().iter().map(|n| n.id()).collect();
        // sort to guarantee each input will be processed by the same node every time
        available_nodes.sort_unstable();

        // println!("available_nodes {:?}", available_nodes);

        // set wanted nodes a.k.a net output
        let mut wanted_nodes: Vec<usize> = net.outputs().iter().map(|n| n.id()).collect();
        // sort to guarantee each output will appear in the same order every time
        wanted_nodes.sort_unstable();
        let wanted_nodes = wanted_nodes;

        // println!("wanted_nodes {:?}", wanted_nodes);

        // gather compute stages by finding computable nodes and required carries until all dependencies are resolved
        while !dependency_graph.is_empty() {
            // setup new compute stage
            let mut stage_matrix: crate::Matrix = Vec::new();
            // setup new transformations
            let mut transformations: crate::Transformations = Vec::new();
            // list of nodes becoming available by compute stage
            let mut next_available_nodes: Vec<usize> = Vec::new();

            for (&dependent_node, dependencies) in dependency_graph.iter() {
                // marker if all dependencies are available
                let mut computable = true;
                // eventual compute vector
                let mut compute_or_carry = vec![f32::NAN; available_nodes.len()];
                // check every dependency
                for &dependency in dependencies {
                    let mut found = false;
                    for (index, &id) in available_nodes.iter().enumerate() {
                        if dependency.start() == id {
                            // add weight to compute vector at position of input
                            compute_or_carry[index] = dependency.weight();
                            found = true;
                        }
                    }
                    // if any dependency is not found the node is not computable yet
                    if !found {
                        computable = false;
                    }
                }
                if computable {
                    // replace NAN with 0.0
                    for n in &mut compute_or_carry {
                        if n.is_nan() {
                            *n = 0.0
                        }
                    }
                    // add vec to compute stage
                    stage_matrix.push(compute_or_carry);
                    // add activation function to stage transformations
                    transformations.push(
                        net.nodes()
                            .iter()
                            .find(|&node| node.id() == dependent_node)
                            .unwrap()
                            .activation(),
                    );
                    // mark node as available in next iteration
                    next_available_nodes.push(dependent_node);
                } else {
                    // figure out carries
                    for (index, &weight) in compute_or_carry.iter().enumerate() {
                        // if there is some partial dependency that is not carried yet
                        if !next_available_nodes
                            .iter()
                            .any(|node| *node == available_nodes[index])
                            && !weight.is_nan()
                        {
                            let mut carry = vec![0.0; available_nodes.len()];
                            carry[index] = 1.0;
                            // add carry vector
                            stage_matrix.push(carry);
                            // add identity function for carried vector
                            transformations.push(|val| val);
                            // add node as available
                            next_available_nodes.push(available_nodes[index]);
                        }
                    }
                }
            }

            // keep any wanted notes if available (output)
            for wanted_node in wanted_nodes.iter() {
                for (index, available_node) in available_nodes.iter().enumerate() {
                    if available_node == wanted_node {
                        // carry only if not carried already
                        if !next_available_nodes
                            .iter()
                            .any(|node| *node == *available_node)
                        {
                            let mut carry = vec![0.0; available_nodes.len()];
                            carry[index] = 1.0;
                            // add carry vector
                            stage_matrix.push(carry);
                            // add identity function for carried vector
                            transformations.push(|val| val);
                            // add node as available
                            next_available_nodes.push(*available_node);
                        }
                    }
                }
            }

            // remove resolved dependencies from dependency graph
            for node in next_available_nodes.iter() {
                dependency_graph.remove(node);
            }

            // if no dependency was removed no progess was made
            if dependency_graph.len() == dependency_count {
                return Err("can't resolve dependencies, net invalid");
            } else {
                dependency_count = dependency_graph.len();
            }

            // println!("next_available_nodes {:?}", next_available_nodes);

            // reorder last stage according to net output order (invalidates next_available_nodes order which wont be used after this point)
            if dependency_graph.is_empty() {
                // println!("stage_matrix {:?}", stage_matrix);

                let mut reordered_matrix = stage_matrix.clone();
                let mut reordered_transformations = transformations.clone();

                let mut matched_wanted_count = 0;

                for ((available_node, column), transformation) in next_available_nodes
                    .iter()
                    .zip(stage_matrix.into_iter())
                    .zip(transformations.into_iter())
                {
                    for (index, wanted_node) in wanted_nodes.iter().enumerate() {
                        if available_node == wanted_node {
                            reordered_matrix[index] = column;
                            reordered_transformations[index] = transformation;
                            matched_wanted_count += 1;
                            break;
                        }
                    }
                }

                if matched_wanted_count < wanted_nodes.len() {
                    return Err(
                        "dependencies resolved but not all outputs computable, net invalid",
                    );
                }

                // println!("reordered_matrix {:?}", reordered_matrix);

                stage_matrix = reordered_matrix;
                transformations = reordered_transformations;
            }

            // add resolved dependencies and transformations to compute stages
            compute_stages.push(stage_matrix);
            stage_transformations.push(transformations);

            // set available nodes for next iteration
            available_nodes = next_available_nodes;
        }

        Ok(super::evaluator::MatrixFeedforwardEvaluator {
            stages: compute_stages
                .into_iter()
                .map(MatrixFeedforwardFabricator::get_matrix)
                .collect(),
            transformations: stage_transformations,
        })
    }
}

#[cfg(test)]
mod tests {
    use nalgebra::dmatrix;

    use super::MatrixFeedforwardFabricator;
    use crate::{
        edges,
        network::{net::Net, Evaluator, Fabricator},
        nodes,
    };

    // tests construction and evaluation of simplest network
    #[test]
    fn simple_net_evaluator_0() {
        let some_net = Net::new(1, 1, nodes!('l', 'l'), edges!(0--0.5->1));

        let evaluator = MatrixFeedforwardFabricator::fabricate(&some_net).unwrap();
        // println!("stages {:?}", evaluator.stages);

        let result = evaluator.evaluate(dmatrix![5.0]);
        // println!("result {:?}", result);

        assert_eq!(result, dmatrix![2.5]);
    }

    // tests input dimension > 1
    #[test]
    fn simple_net_evaluator_1() {
        let some_net = Net::new(
            2,
            1,
            nodes!('l', 'l', 'l'),
            edges!(
                0--0.5->2,
                1--0.5->2
            ),
        );

        let evaluator = MatrixFeedforwardFabricator::fabricate(&some_net).unwrap();
        // println!("stages {:?}", evaluator.stages);

        let result = evaluator.evaluate(dmatrix![5.0, 5.0]);
        // println!("result {:?}", result);

        assert_eq!(result, dmatrix![5.0]);
    }

    // test linear chaining of edges
    #[test]
    fn simple_net_evaluator_2() {
        let some_net = Net::new(
            1,
            1,
            nodes!('l', 'l', 'l'),
            edges!(
                0--0.5->1,
                1--0.5->2
            ),
        );

        let evaluator = MatrixFeedforwardFabricator::fabricate(&some_net).unwrap();
        // println!("stages {:?}", evaluator.stages);

        let result = evaluator.evaluate(dmatrix![5.0]);
        // println!("result {:?}", result);

        assert_eq!(result, dmatrix![1.25]);
    }

    // test construction of carry for later needs
    #[test]
    fn simple_net_evaluator_3() {
        let some_net = Net::new(
            1,
            1,
            nodes!('l', 'l', 'l'),
            edges!(
                0--0.5->1,
                1--0.5->2,
                0--0.5->2
            ),
        );

        let evaluator = MatrixFeedforwardFabricator::fabricate(&some_net).unwrap();
        // println!("stages {:?}", evaluator.stages);

        let result = evaluator.evaluate(dmatrix![5.0]);
        // println!("result {:?}", result);

        assert_eq!(result, dmatrix![3.75]);
    }

    // test construction of carry for early result with dedup carry
    #[test]
    fn simple_net_evaluator_4() {
        let some_net = Net::new(
            1,
            2,
            nodes!('l', 'l', 'l', 'l'),
            edges!(
                0--0.5->1,
                1--0.5->2,
                0--0.5->3,
                0--0.5->2
            ),
        );

        let evaluator = MatrixFeedforwardFabricator::fabricate(&some_net).unwrap();

        let result = evaluator.evaluate(dmatrix![5.0]);

        assert_eq!(result, dmatrix![3.75, 2.5]);
    }

    // test construction of carry for early result flipped order
    #[test]
    fn simple_net_evaluator_5() {
        let some_net = Net::new(
            1,
            2,
            nodes!('l', 'l', 'l', 'l'),
            edges!(
                0--0.5->1,
                1--0.5->3,
                0--0.5->2
            ),
        );

        let evaluator = MatrixFeedforwardFabricator::fabricate(&some_net).unwrap();
        // println!("stages {:?}", evaluator.stages);

        let result = evaluator.evaluate(dmatrix![5.0]);
        // println!("result {:?}", result);

        assert_eq!(result, dmatrix![2.5, 1.25]);
    }

    // test unconnected net
    #[test]
    fn simple_net_evaluator_6() {
        let some_net = Net::new(1, 1, nodes!('l', 'l'), Vec::new());

        if let Err(message) = MatrixFeedforwardFabricator::fabricate(&some_net) {
            assert_eq!(message, "no edges present, net invalid");
        } else {
            unreachable!();
        }
    }

    // test uncomputable output
    #[test]
    fn simple_net_evaluator_7() {
        let some_net = Net::new(1, 1, nodes!('l', 'l', 'l'), edges!(0--0.5->1));

        if let Err(message) = MatrixFeedforwardFabricator::fabricate(&some_net) {
            assert_eq!(
                message,
                "dependencies resolved but not all outputs computable, net invalid"
            );
        } else {
            unreachable!();
        }
    }

    // test uncomputable output
    #[test]
    fn simple_net_evaluator_8() {
        let some_net = Net::new(1, 1, nodes!('l', 'l', 'l'), edges!(1--0.5->2));

        if let Err(message) = MatrixFeedforwardFabricator::fabricate(&some_net) {
            assert_eq!(message, "can't resolve dependencies, net invalid");
        } else {
            unreachable!();
        }
    }

    #[test]
    fn simple_net_evaluator_9() {
        let some_net = Net::new(
            2,
            1,
            nodes!('l', 'l', 'l'),
            edges!(
                0--0.5->2,
                1--0.0->2
            ),
        );

        let evaluator = MatrixFeedforwardFabricator::fabricate(&some_net).unwrap();
        // println!("stages {:?}", evaluator.stages);

        let result = evaluator.evaluate(dmatrix![5.0, 5.0]);
        // println!("result {:?}", result);

        assert_eq!(result, dmatrix![2.5]);
    }
}
