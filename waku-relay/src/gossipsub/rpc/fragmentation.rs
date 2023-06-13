use prost::Message;

use crate::gossipsub::rpc::{ControlMessageProto, RpcProto};

pub enum FragmentationError {
    MessageTooLarge,
}

// If a message is too large to be sent as-is, this attempts to fragment it into smaller RPC
// messages to be sent.
pub fn fragment_rpc_message(
    rpc: RpcProto,
    max_size: usize,
) -> Result<Vec<RpcProto>, FragmentationError> {
    if rpc.encoded_len() < max_size {
        return Ok(vec![rpc]);
    }

    let new_rpc = RpcProto {
        subscriptions: Vec::new(),
        publish: Vec::new(),
        control: None,
    };

    let mut rpc_list = vec![new_rpc.clone()];

    // Gets an RPC if the object size will fit, otherwise create a new RPC. The last element
    // will be the RPC to add an object.
    macro_rules! create_or_add_rpc {
        ($object_size: ident ) => {
            let list_index = rpc_list.len() - 1; // the list is never empty

            // create a new RPC if the new object plus 5% of its size (for length prefix
            // buffers) exceeds the max transmit size.
            if rpc_list[list_index].encoded_len() + (($object_size as f64) * 1.05) as usize
                > max_size
                && rpc_list[list_index] != new_rpc
            {
                // create a new rpc and use this as the current
                rpc_list.push(new_rpc.clone());
            }
        };
    }

    macro_rules! add_item {
        ($object: ident, $type: ident ) => {
            let object_size = $object.encoded_len();

            if object_size + 2 > max_size {
                // This should not be possible. All received and published messages have already
                // been vetted to fit within the size.
                log::error!("Individual message too large to fragment");
                return Err(FragmentationError::MessageTooLarge);
            }

            create_or_add_rpc!(object_size);
            rpc_list
                .last_mut()
                .expect("Must have at least one element")
                .$type
                .push($object.clone());
        };
    }

    // Add messages until the limit
    for message in &rpc.publish {
        add_item!(message, publish);
    }
    for subscription in &rpc.subscriptions {
        add_item!(subscription, subscriptions);
    }

    // handle the control messages. If all are within the max_transmit_size, send them without
    // fragmenting, otherwise, fragment the control messages
    let empty_control = ControlMessageProto::default();
    if let Some(control) = rpc.control.as_ref() {
        if control.encoded_len() + 2 > max_size {
            // fragment the RPC
            for ihave in &control.ihave {
                let len = ihave.encoded_len();
                create_or_add_rpc!(len);
                rpc_list
                    .last_mut()
                    .expect("Always an element")
                    .control
                    .get_or_insert_with(|| empty_control.clone())
                    .ihave
                    .push(ihave.clone());
            }
            for iwant in &control.iwant {
                let len = iwant.encoded_len();
                create_or_add_rpc!(len);
                rpc_list
                    .last_mut()
                    .expect("Always an element")
                    .control
                    .get_or_insert_with(|| empty_control.clone())
                    .iwant
                    .push(iwant.clone());
            }
            for graft in &control.graft {
                let len = graft.encoded_len();
                create_or_add_rpc!(len);
                rpc_list
                    .last_mut()
                    .expect("Always an element")
                    .control
                    .get_or_insert_with(|| empty_control.clone())
                    .graft
                    .push(graft.clone());
            }
            for prune in &control.prune {
                let len = prune.encoded_len();
                create_or_add_rpc!(len);
                rpc_list
                    .last_mut()
                    .expect("Always an element")
                    .control
                    .get_or_insert_with(|| empty_control.clone())
                    .prune
                    .push(prune.clone());
            }
        } else {
            let len = control.encoded_len();
            create_or_add_rpc!(len);
            rpc_list.last_mut().expect("Always an element").control = Some(control.clone());
        }
    }

    Ok(rpc_list)
}
