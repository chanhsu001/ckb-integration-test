mod basic;
mod case_options;
mod rfc0036;
mod rfc0035;
mod rfc0034;
mod rfc0032;
mod rfc0221;
mod rfc0222;
mod rfc0223;
mod rfc0224;
mod rfc0240;

use case_options::CaseOptions;
use ckb_testkit::Node;
use ckb_testkit::Nodes;

pub fn all_cases() -> Vec<Box<dyn Case>> {
    vec![
        Box::new(basic::networking::BasicNetworking),
        Box::new(rfc0221::before_switch::RFC0221BeforeSwitch),
        Box::new(rfc0221::after_switch::RFC0221AfterSwitch),
        Box::new(rfc0222::before_switch::RFC0222BeforeSwitch),
        Box::new(rfc0222::after_switch::RFC0222AfterSwitch),
        Box::new(rfc0223::before_switch::RFC0223BeforeSwitch),
        Box::new(rfc0223::after_switch::RFC0223AfterSwitch),
        Box::new(rfc0224::before_switch::RFC0224BeforeSwitch),
        Box::new(rfc0224::after_switch::RFC0224AfterSwitch),
        Box::new(rfc0234::before_switch::RFC0234BeforeSwitch),
        Box::new(rfc0234::relay_transaction::RFC0234AfterSwitchRelayTransaction),
        Box::new(rfc0234::connection::RFC0234AfterSwitchConnection),
        Box::new(rfc0234::discovery::RFC0234AfterSwitchDiscovery),
        Box::new(rfc0240::before_switch::RFC0240BeforeSwitch),
        Box::new(rfc0240::after_switch::RFC0240AfterSwitch),
        Box::new(rfc0036::v2019::RFC0036V2019),
        Box::new(rfc0036::v2021::RFC0036V2021),
        Box::new(rfc0035::v2019::RFC0035V2019),
        Box::new(rfc0035::relay_transaction::RFC0035V2021RelayTransaction),
        Box::new(rfc0035::connection::RFC0035V2021Connection),
        Box::new(rfc0035::discovery::RFC0035V2021Discovery),
        Box::new(rfc0034::rfc0034::RFC0034),
        Box::new(rfc0032::rfc0032::RFC0032),
    ]
}

pub fn run_case(case: Box<dyn Case>) {
    ckb_testkit::LOG_TARGET.with(|c| {
        *c.borrow_mut() = case.case_name().to_string();
    });

    ckb_testkit::info!("********** START **********");
    let nodes = case.before_run();
    case.run(nodes);
    ckb_testkit::info!("********** END **********");
}

pub trait Case: Send {
    fn case_name(&self) -> &str {
        case_name(self)
    }

    fn case_options(&self) -> CaseOptions;

    fn before_run(&self) -> Nodes {
        let case_name = self.case_name();
        let case_options = self.case_options();
        let mut nodes = ::std::collections::HashMap::new();
        let mut first_node_name = None;
        for node_options in case_options.node_options.iter() {
            let mut node = Node::init(
                case_name,
                node_options.clone(),
                node_options.ckb_binary == *crate::CKB2021.read().unwrap(),
            );
            let node_name = node.node_name().to_string();
            node.start();
            nodes.insert(node_name.clone(), node);
            if first_node_name.is_none() {
                first_node_name = Some(node_name);
            }
        }
        let nodes = Nodes::from(nodes);
        if case_options.make_all_nodes_connected_and_synced {
            for node in nodes.nodes() {
                node.mine(1);
            }
            nodes.p2p_connect();
            let any_node = nodes.get_node(first_node_name.as_ref().unwrap());
            any_node.mine(1);
            nodes.waiting_for_sync().expect("waiting for sync");
        } else {
            if case_options.make_all_nodes_connected {
                nodes.p2p_connect();
            }
            if case_options.make_all_nodes_synced {
                let any_node = nodes.get_node(first_node_name.as_ref().unwrap());
                any_node.mine(1);
                let tip_block = any_node.get_tip_block();
                for node in nodes.nodes() {
                    if node.node_name() != any_node.node_name() {
                        node.submit_block(&tip_block);
                    }
                }
                nodes.waiting_for_sync().expect("waiting for sync");
            }
        }
        nodes
    }

    fn run(&self, nodes: Nodes);
}

fn case_name<T: ?Sized>(_: &T) -> &str {
    let type_name = ::std::any::type_name::<T>();
    type_name.split_terminator("::").last().unwrap()
}
