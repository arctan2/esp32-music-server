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
	musicList: van.state<Music[]>([]),
	curPlaying: van.state<null | number>(null),
	async fetchMusicList() {
		const res = await fetch("http://192.168.0.107:8000/list/music");
		const data = await res.text();
		const doc = document.implementation.createHTMLDocument("music_list");
		doc.body.innerHTML = data;
		this.musicList.val = parseList(doc);
	},
	getCurrentPlaying(): null | Music {
		return this.curPlaying.val === null ? null : this.musicList.val[this.curPlaying.val];
	},
	handleMusicClick(musicIdx: number) {
		this.curPlaying.val = musicIdx;
	},
	render() {
		return this.musicList.val.map((music, idx) => {
			let iconClassList = ["icon"];
			if(idx === this.curPlaying.val) {
				iconClassList.push("playing");
			}
			return div({ className: "music", onclick: () => this.handleMusicClick(idx) },
				div({ className: iconClassList.join(" ") },
					div({ className: "play-icon" })
				),
				div({ className: "music-details" },
					h2(music.name),
					div(music.id),
					div(music.size, "B")
				)
			)
		});
	},
	mount() {
		return this.render();
	}
}
