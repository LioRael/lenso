use super::*;

pub(super) fn resolve_parts(
    module_instances: &[ModuleInstancePlan],
    capability_bindings: &[CapabilityBinding],
) -> Result<(Vec<ModuleInstancePlan>, Vec<CapabilityBinding>), PlanResolutionError> {
    let (instances, instance_indices) = normalize_instances(module_instances)?;
    let grouped_bindings = group_bindings(&instances, &instance_indices, capability_bindings)?;
    validate_requirement_cardinality(&instances, &grouped_bindings)?;
    validate_activation_cycles(&instances, &grouped_bindings)?;
    Ok((instances, order_bindings(grouped_bindings)))
}

fn normalize_instances(
    module_instances: &[ModuleInstancePlan],
) -> Result<(Vec<ModuleInstancePlan>, BTreeMap<String, usize>), PlanResolutionError> {
    let mut instances = module_instances.to_vec();
    sort_module_instances(&mut instances);

    let mut instance_indices = BTreeMap::new();
    for (index, instance) in instances.iter().enumerate() {
        if instance_indices
            .insert(instance.instance_key.clone(), index)
            .is_some()
        {
            return Err(PlanResolutionError::DuplicateModuleInstance {
                instance_key: instance.instance_key.clone(),
            });
        }
        validate_instance_declarations(instance)?;
        instance.restart_policy.validate(&instance.instance_key)?;
    }
    Ok((instances, instance_indices))
}

fn group_bindings(
    instances: &[ModuleInstancePlan],
    instance_indices: &BTreeMap<String, usize>,
    capability_bindings: &[CapabilityBinding],
) -> Result<BTreeMap<(String, String), Vec<CapabilityBinding>>, PlanResolutionError> {
    let mut grouped_bindings = BTreeMap::new();
    for binding in capability_bindings {
        validate_binding(instances, instance_indices, binding)?;
        grouped_bindings
            .entry((
                binding.consumer_instance.clone(),
                binding.capability_id.clone(),
            ))
            .or_insert_with(Vec::new)
            .push(binding.clone());
    }
    Ok(grouped_bindings)
}

fn validate_binding(
    instances: &[ModuleInstancePlan],
    instance_indices: &BTreeMap<String, usize>,
    binding: &CapabilityBinding,
) -> Result<(), PlanResolutionError> {
    let Some(&consumer_index) = instance_indices.get(&binding.consumer_instance) else {
        return Err(PlanResolutionError::InvalidConsumerReference {
            consumer_instance: binding.consumer_instance.clone(),
            capability_id: binding.capability_id.clone(),
        });
    };
    let consumer = &instances[consumer_index];
    let Some(requirement) = consumer
        .required_capabilities
        .iter()
        .find(|requirement| requirement.capability_id == binding.capability_id)
    else {
        return Err(PlanResolutionError::UndeclaredCapabilityRequirement {
            consumer_instance: binding.consumer_instance.clone(),
            capability_id: binding.capability_id.clone(),
        });
    };

    let Some(&provider_index) = instance_indices.get(&binding.provider_instance) else {
        return Err(PlanResolutionError::InvalidProviderReference {
            consumer_instance: binding.consumer_instance.clone(),
            capability_id: binding.capability_id.clone(),
            provider_instance: binding.provider_instance.clone(),
        });
    };
    let provider = &instances[provider_index];
    let Some(endpoint) = provider
        .provided_capabilities
        .iter()
        .find(|endpoint| endpoint.capability_id == binding.capability_id)
    else {
        return Err(PlanResolutionError::InvalidProviderReference {
            consumer_instance: binding.consumer_instance.clone(),
            capability_id: binding.capability_id.clone(),
            provider_instance: binding.provider_instance.clone(),
        });
    };

    if endpoint.descriptor_version != requirement.descriptor_version {
        return Err(PlanResolutionError::IncompatibleCapabilityVersion {
            consumer_instance: binding.consumer_instance.clone(),
            capability_id: binding.capability_id.clone(),
            required: requirement.descriptor_version.clone(),
            provided: endpoint.descriptor_version.clone(),
            provider_instance: binding.provider_instance.clone(),
        });
    }
    if binding.descriptor_version != requirement.descriptor_version {
        return Err(PlanResolutionError::IncompatibleCapabilityVersion {
            consumer_instance: binding.consumer_instance.clone(),
            capability_id: binding.capability_id.clone(),
            required: requirement.descriptor_version.clone(),
            provided: binding.descriptor_version.clone(),
            provider_instance: binding.provider_instance.clone(),
        });
    }
    if consumer.execution_lane != provider.execution_lane && !endpoint.cross_lane_transfer {
        return Err(PlanResolutionError::CrossLaneTransferUnsupported {
            consumer_instance: binding.consumer_instance.clone(),
            provider_instance: binding.provider_instance.clone(),
            capability_id: binding.capability_id.clone(),
        });
    }
    if consumer.execution_lane != provider.execution_lane {
        for operation in &endpoint.operations {
            let interaction = endpoint
                .operation_kind(operation)
                .expect("iterated Operation is declared by this endpoint");
            if interaction != CapabilityOperationKind::Request {
                return Err(PlanResolutionError::CrossLaneInteractionUnsupported {
                    capability_id: endpoint.capability_id.clone(),
                    operation: operation.clone(),
                    interaction,
                });
            }
        }
    }
    if binding.has_explicit_admission() {
        for operation in &endpoint.operations {
            binding
                .admission()
                .validate(&endpoint.capability_id, operation)?;
        }
    }
    Ok(())
}

fn validate_requirement_cardinality(
    instances: &[ModuleInstancePlan],
    grouped_bindings: &BTreeMap<(String, String), Vec<CapabilityBinding>>,
) -> Result<(), PlanResolutionError> {
    for instance in instances {
        for endpoint in &instance.provided_capabilities {
            validate_endpoint_admission(endpoint)?;
        }
        for requirement in &instance.required_capabilities {
            let key = (
                instance.instance_key.clone(),
                requirement.capability_id.clone(),
            );
            let bindings = grouped_bindings.get(&key).map_or(&[][..], Vec::as_slice);
            match (requirement.cardinality, bindings.len()) {
                (CapabilityCardinality::One, 0) => {
                    return Err(PlanResolutionError::MissingOneBinding {
                        consumer_instance: instance.instance_key.clone(),
                        capability_id: requirement.capability_id.clone(),
                    });
                }
                (CapabilityCardinality::One, providers) if providers > 1 => {
                    return Err(PlanResolutionError::AmbiguousOneBinding {
                        consumer_instance: instance.instance_key.clone(),
                        capability_id: requirement.capability_id.clone(),
                        providers,
                    });
                }
                (CapabilityCardinality::Optional, providers) if providers > 1 => {
                    return Err(PlanResolutionError::AmbiguousOptionalBinding {
                        consumer_instance: instance.instance_key.clone(),
                        capability_id: requirement.capability_id.clone(),
                        providers,
                    });
                }
                _ => {}
            }

            let mut provider_keys = BTreeSet::new();
            for binding in bindings {
                if !provider_keys.insert(binding.provider_instance.as_str()) {
                    return Err(PlanResolutionError::DuplicateBinding {
                        consumer_instance: binding.consumer_instance.clone(),
                        capability_id: binding.capability_id.clone(),
                        provider_instance: binding.provider_instance.clone(),
                    });
                }
            }
        }
    }
    Ok(())
}

fn validate_endpoint_admission(
    endpoint: &CapabilityEndpointPlan,
) -> Result<(), PlanResolutionError> {
    for operation in &endpoint.operations {
        if let Some(admission) = endpoint.operation_admission(operation) {
            admission.validate(&endpoint.capability_id, operation)?;
        }
    }
    for operation in endpoint.operation_admissions.keys() {
        if !endpoint
            .operations
            .iter()
            .any(|declared| declared == operation)
        {
            return Err(PlanResolutionError::UnknownAdmissionOperation {
                capability_id: endpoint.capability_id.clone(),
                operation: operation.clone(),
            });
        }
    }
    for operation in endpoint.operation_kinds.keys() {
        if !endpoint
            .operations
            .iter()
            .any(|declared| declared == operation)
        {
            return Err(PlanResolutionError::UnknownOperationInteraction {
                capability_id: endpoint.capability_id.clone(),
                operation: operation.clone(),
            });
        }
    }
    Ok(())
}

fn order_bindings(
    grouped_bindings: BTreeMap<(String, String), Vec<CapabilityBinding>>,
) -> Vec<CapabilityBinding> {
    let mut ordered_bindings = Vec::new();
    for (_, mut bindings) in grouped_bindings {
        bindings.sort_by(|left, right| {
            left.provider_instance
                .cmp(&right.provider_instance)
                .then_with(|| left.descriptor_version.cmp(&right.descriptor_version))
        });
        for (provider_order, binding) in bindings.into_iter().enumerate() {
            ordered_bindings.push(binding.with_provider_order(provider_order));
        }
    }
    ordered_bindings
}

fn validate_activation_cycles(
    instances: &[ModuleInstancePlan],
    grouped_bindings: &BTreeMap<(String, String), Vec<CapabilityBinding>>,
) -> Result<(), PlanResolutionError> {
    let bindings = grouped_bindings
        .values()
        .flat_map(|bindings| bindings.iter())
        .cloned()
        .collect::<Vec<_>>();
    activation_order_for(instances, &bindings)
        .map(|_| ())
        .map_err(|instances| PlanResolutionError::ActivationCycle { instances })
}

pub(super) fn activation_order_for(
    instances: &[ModuleInstancePlan],
    bindings: &[CapabilityBinding],
) -> Result<Vec<String>, Vec<String>> {
    let mut indegrees: BTreeMap<String, usize> = instances
        .iter()
        .map(|instance| (instance.instance_key.clone(), 0))
        .collect();
    let mut dependents: BTreeMap<String, BTreeSet<String>> = instances
        .iter()
        .map(|instance| (instance.instance_key.clone(), BTreeSet::new()))
        .collect();

    for binding in bindings {
        let consumers = dependents
            .get_mut(&binding.provider_instance)
            .expect("provider Instance was indexed before dependency validation");
        if consumers.insert(binding.consumer_instance.clone()) {
            *indegrees
                .get_mut(&binding.consumer_instance)
                .expect("consumer Instance was indexed before dependency validation") += 1;
        }
    }

    let mut ready: BTreeSet<String> = indegrees
        .iter()
        .filter(|(_, indegree)| **indegree == 0)
        .map(|(instance, _)| instance.clone())
        .collect();
    let mut order = Vec::with_capacity(instances.len());
    while let Some(instance) = ready.pop_first() {
        order.push(instance.clone());
        if let Some(consumers) = dependents.get(&instance) {
            for consumer in consumers {
                let indegree = indegrees
                    .get_mut(consumer)
                    .expect("consumer Instance was indexed before dependency validation");
                *indegree -= 1;
                if *indegree == 0 {
                    ready.insert(consumer.clone());
                }
            }
        }
    }

    if order.len() == instances.len() {
        Ok(order)
    } else {
        Err(indegrees
            .into_iter()
            .filter(|(_, indegree)| *indegree > 0)
            .map(|(instance, _)| instance)
            .collect())
    }
}

fn validate_instance_declarations(
    instance: &ModuleInstancePlan,
) -> Result<(), PlanResolutionError> {
    if instance.entrypoint.trim().is_empty() {
        return Err(PlanResolutionError::InvalidModuleEntrypoint {
            instance_key: instance.instance_key.clone(),
        });
    }
    let mut provided = BTreeSet::new();
    for endpoint in &instance.provided_capabilities {
        if !provided.insert(endpoint.capability_id.as_str()) {
            return Err(PlanResolutionError::DuplicateProvidedCapability {
                provider_instance: instance.instance_key.clone(),
                capability_id: endpoint.capability_id.clone(),
            });
        }
        let mut operations = BTreeSet::new();
        for operation in &endpoint.operations {
            if !operations.insert(operation.as_str()) {
                return Err(PlanResolutionError::DuplicateOperation {
                    provider_instance: instance.instance_key.clone(),
                    capability_id: endpoint.capability_id.clone(),
                    operation: operation.clone(),
                });
            }
        }
    }

    let mut required = BTreeSet::new();
    for requirement in &instance.required_capabilities {
        if !required.insert(requirement.capability_id.as_str()) {
            return Err(PlanResolutionError::DuplicateRequiredCapability {
                consumer_instance: instance.instance_key.clone(),
                capability_id: requirement.capability_id.clone(),
            });
        }
    }
    Ok(())
}

pub(super) fn sort_module_instances(instances: &mut [ModuleInstancePlan]) {
    instances.sort_by(|left, right| left.instance_key.cmp(&right.instance_key));
}

pub(super) fn sorted_execution_lanes(lanes: &[ExecutionLanePlan]) -> Vec<ExecutionLanePlan> {
    let mut lanes = lanes.to_vec();
    lanes.sort_by(|left, right| left.id().cmp(right.id()));
    lanes
}

pub(super) fn validate_execution_lanes(
    lanes: &[ExecutionLanePlan],
    instances: &[ModuleInstancePlan],
) -> Result<(), PlanResolutionError> {
    if lanes.is_empty() {
        return Err(PlanResolutionError::MissingExecutionLane);
    }
    let mut declared = BTreeSet::new();
    for lane in lanes {
        if lane.id().as_str().trim().is_empty() {
            return Err(PlanResolutionError::InvalidExecutionLane {
                execution_lane: lane.id().to_string(),
            });
        }
        if !declared.insert(lane.id()) {
            return Err(PlanResolutionError::DuplicateExecutionLane {
                execution_lane: lane.id().to_string(),
            });
        }
    }
    for instance in instances {
        if !declared.contains(instance.execution_lane()) {
            return Err(PlanResolutionError::UndeclaredExecutionLane {
                instance_key: instance.instance_key().to_owned(),
                execution_lane: instance.execution_lane().to_string(),
            });
        }
    }
    Ok(())
}

pub(super) fn sort_bindings(bindings: &mut [CapabilityBinding]) {
    bindings.sort_by(|left, right| {
        left.consumer_instance
            .cmp(&right.consumer_instance)
            .then_with(|| left.capability_id.cmp(&right.capability_id))
            .then_with(|| left.provider_instance.cmp(&right.provider_instance))
            .then_with(|| left.provider_order.cmp(&right.provider_order))
    });
}
