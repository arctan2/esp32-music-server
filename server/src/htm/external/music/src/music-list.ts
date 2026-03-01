import van from "vanjs-core";
import "./music-list.css";

const { div, h2 } = van.tags;

export type Music = { name: string, id: string, size: number };

function parseList(doc: Document) {
	const listEl: HTMLDivElement = doc.getElementById("list")! as HTMLDivElement;

	let musicList: Music[] = [];

	for(let child of listEl.children) {
		if(child.tagName !== 'DIV') continue;

		const size = parseInt((child.children[0] as HTMLSpanElement).innerText);
		const [id, name] = (child.children[1] as HTMLSpanElement).innerText.split(";");

		musicList.push({ size, id, name });
	}

	return musicList;
}

export const MusicList = {
	msg: van.state<string>("Loading..."),
	musicList: van.state<Music[]>([]),
	curPlaying: van.state<null | Music>(null),
	async fetchMusicList() {
		try {
			const res = await fetch("http://192.168.0.107:8000/list/music");
			const data = await res.text();
			const doc = document.implementation.createHTMLDocument("music_list");
			doc.body.innerHTML = data;
			this.musicList.val = parseList(doc);
			this.msg.val = "";
		} catch(e) {
			this.msg.val = String(e);
		}
	},
	handleMusicClick(musicIdx: number) {
		const music = this.musicList.val[musicIdx];
		if(music.id !== this.curPlaying.val?.id) {
			this.curPlaying.val = { ...music };
		}
	},
	render() {
		if(this.msg.val !== "") {
			return div({ className: "msg" },
				this.msg.val
			)
		}

		if(this.musicList.val.length === 0) {
			return div({ className: "msg" },
				"No music found :("
			)
		}

		return this.musicList.val.map((music, idx) => {
			let iconClassList = ["icon"];
			let playIconClassList = ["play-icon", "play"];
			if(music.id === this.curPlaying.val?.id) {
				iconClassList.push("playing");
			}

			return div({ className: "music", onclick: () => this.handleMusicClick(idx) },
				div({ className: iconClassList.join(" ") },
					div({ className: playIconClassList.join(" ") })
				),
				div({ className: "music-details" },
					h2(music.name),
					div(music.id),
					div(music.size, " B")
				)
			)
		});
	},
}
