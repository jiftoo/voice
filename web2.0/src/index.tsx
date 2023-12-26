/* @refresh reload */
import {render} from "solid-js/web";
import App from "./App";
import "./index.css";
import {Router, Route} from "@solidjs/router";
import Upload from "./screens/Upload";
import Task from "./screens/Task";

const root = document.getElementById("root");
render(
	() => (
		<Router root={App}>
			<Route path="/" component={Upload} />
			<Route path="/task/:id" component={Task} />
		</Router>
	),
	root!
);
