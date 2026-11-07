import MezotsApp from "@/components/MezotsApp";

/**
 * The Mezots interface.
 *
 * The whole application is a client component: it talks to a node over a
 * WebSocket and renders whatever that node knows about the network.
 */
export default function Home() {
  return <MezotsApp />;
}
