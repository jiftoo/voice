import {useParams} from "@solidjs/router";

export default function Task() {
	const params = useParams();

	return <div>{params.id}</div>;
}
