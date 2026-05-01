import { useEffect, useState, useRef } from 'react';
import { FrontendSession, FrontendMessageKind, WebSocketMessage } from './types';

export const App = () => {
  const [session, setSession] = useState<FrontendSession | null>(null);
  const [input, setInput] = useState('');
  const ws = useRef<WebSocket | null>(null);

  useEffect(() => {
    ws.current = new WebSocket('ws://localhost:3000/ws');

    ws.current.onmessage = (event) => {
      const msg: WebSocketMessage = JSON.parse(event.data);
      if (msg.type === 'state_update') {
        setSession(msg.payload as FrontendSession);
      }
    };

    return () => ws.current?.close();
  }, []);

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (ws.current && input.trim()) {
      ws.current.send(JSON.stringify({
        v: 1,
        type: 'input',
        payload: { content: input }
      }));
      setInput('');
    }
  };

  if (!session) return <div>Connecting...</div>;

  return (
    <div className="app">
      <div className="messages">
        {session.messages.map((msg, i) => (
          <div key={i} className={`message ${msg.kind.toLowerCase()}`}>
            <span className="prefix">{msg.kind}: </span>
            {msg.content}
          </div>
        ))}
        {session.pending_request && <div className="pending">...</div>}
      </div>
      <form onSubmit={handleSubmit}>
        <input 
          value={input} 
          onChange={(e) => setInput(e.target.value)} 
          placeholder="Type a message..." 
        />
        <button type="submit">Send</button>
      </form>
    </div>
  );
};
