pub mod mouse;
pub mod pci;
pub mod xhci;

use crate::log_ok;

pub fn init() {
    xhci::self_test();
    mouse::init();
    let controllers = pci::find_usb_controllers();

    if controllers.is_empty() {
        log_ok!("USB", "Init", "No USB host controller found on the PCI bus");
        return;
    }

    for c in &controllers {
        log_ok!(
            "USB",
            "Init",
            "Found {} controller at {:02x}:{:02x}.{} (BAR0 base {:#x})",
            c.kind.name(),
            c.location.bus,
            c.location.device,
            c.location.function,
            c.mmio_base
        );
    }

    let mut found_xhci = false;
    for xhci_ctrl in controllers
        .iter()
        .filter(|c| c.kind == pci::ControllerKind::Xhci)
    {
        found_xhci = true;
        xhci::probe(xhci_ctrl);
    }
    if !found_xhci {
        log_ok!(
            "USB",
            "Init",
            "No xHCI controller among them -- only xHCI is supported so far, stopping here"
        );
    }
}
