import { Modal } from "../../components/ui/Modal";
import { closeImportWizard, ui } from "../../lib/ui";
import { ImportFlow } from "./ImportFlow";

export function ImportWizard() {
  const open = ui.use((s) => s.importOpen);
  // Mounting the flow only while open re-runs detection each time the wizard is opened.
  return (
    <Modal open={open} title="Import from another browser" onClose={closeImportWizard} width={560}>
      {open && <ImportFlow onClose={closeImportWizard} />}
    </Modal>
  );
}
