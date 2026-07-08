using System;
using System.Windows.Forms;

// Synthetic WinForms navigation fixture — exercises the `navigates_to` arm
// (EmitNavArm). GENERIC screen names only: the harvester machinery is agnostic
// and real corpora are pointed at via a path, never vendored (README
// "Provenance / non-vendoring"). Expected edges out of MainScreen:
//   MainScreen -> OrderScreen     (one-liner  new X().Show())
//   MainScreen -> SettingsScreen  (one-liner  new X().ShowDialog())
//   MainScreen -> CustomerScreen  (two-stmt   var f = new X(); ...; f.Show())
// SaveFileDialog is a framework CommonDialog and must NOT produce an edge.
namespace NavShapes
{
    public class MainScreen : Form
    {
        // one-liner: new TargetForm().Show()
        private void OpenOrders_Click(object sender, EventArgs e)
        {
            new OrderScreen().Show();
        }

        // one-liner modal: new TargetForm().ShowDialog()
        private void OpenSettings_Click(object sender, EventArgs e)
        {
            new SettingsScreen().ShowDialog();
        }

        // two-statement: local tracked, then .Show()
        private void OpenCustomer_Click(object sender, EventArgs e)
        {
            var f = new CustomerScreen();
            f.Text = "Customer";
            f.Show();
        }

        // framework CommonDialog — must NOT emit a navigates_to edge
        private void Save_Click(object sender, EventArgs e)
        {
            var dlg = new SaveFileDialog();
            dlg.ShowDialog();
        }
    }

    public class OrderScreen : Form { }

    public class SettingsScreen : Form { }

    public class CustomerScreen : Form { }
}
