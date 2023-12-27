import "./Navbar.css";
import Switch from "./components/Switch";
import {GLOBAL_STATE} from "./globalState";
import {useNavigate} from "@solidjs/router";

export default function Navbar() {
	const navigate = useNavigate();
	return (
		<nav class="rounded">
			<div id="logo" onClick={() => navigate("/")}>
				Voice
			</div>
			<Switch reverse value={GLOBAL_STATE.premium[0]()} onChange={GLOBAL_STATE.premium[1]}>
				Premium mode
			</Switch>
		</nav>
	);
}
