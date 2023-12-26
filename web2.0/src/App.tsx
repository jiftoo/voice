import "./App.css";
import "./Main.css";
import Navbar from "./Navbar";
import {useLocation, RouteSectionProps} from "@solidjs/router";

export default function App(props: RouteSectionProps) {
	const location = useLocation();
	return (
		<>
			<Navbar />
			<div class="main-content rounded" classList={{"no-bottom-padding": location.pathname === "/"}}>
				{props.children}
			</div>
		</>
	);
}
