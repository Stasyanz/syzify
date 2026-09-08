import { useNavigate } from "react-router";
import { Puzzle } from "lucide-react";

/** The Settings entry to the plugin registry: sideload, enable, and — for
 * a sync plugin — the Sync button on its card. Hidden while the extension
 * work was under way (6ab5a14); back with the sync plugins. */
export function PluginsCard() {
  const navigate = useNavigate();
  return (
    <section className="card">
      <h3>Plugins</h3>
      <div className="set-row">
        <div>
          <div className="sl">Plugins &amp; extensions</div>
          <div className="sd">Manage installed plugins, their permissions and syncs</div>
        </div>
        <button onClick={() => navigate("/plugins")} className="btn ghost">
          <Puzzle size={15} />
          Manage
        </button>
      </div>
    </section>
  );
}
