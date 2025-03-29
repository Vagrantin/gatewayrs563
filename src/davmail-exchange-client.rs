<t:Folder>
                             <t:DisplayName>{}</t:DisplayName>
                           </t:Folder>
                         </t:Folders>"#, folder_name)
            },
        };
        
        // Build the EWS GetFolder request
        let body = format!(r#"<?xml version="1.0" encoding="utf-8"?>
            <soap:Envelope xmlns:soap="http://schemas.xmlsoap.org/soap/envelope/"
                           xmlns:t="http://schemas.microsoft.com/exchange/services/2006/types">
              <soap:Body>
                <GetFolder xmlns="http://schemas.microsoft.com/exchange/services/2006/messages"
                          xmlns:t="http://schemas.microsoft.com/exchange/services/2006/types">
                  <FolderShape>
                    <t:BaseShape>Default</t:BaseShape>
                    <t:AdditionalProperties>
                      <t:FieldURI FieldURI="folder:TotalCount"/>
                      <t:FieldURI FieldURI="folder:UnreadCount"/>
                    </t:AdditionalProperties>
                  </FolderShape>
                  <FolderIds>
                    {}
                  </FolderIds>
                </GetFolder>
              </soap:Body>
            </soap:Envelope>"#, folder_id);
        
        // Send the request
        let response = self.client
            .post(format!("{}/EWS/Exchange.asmx", self.base_url))
            .headers(headers)
            .body(body)
            .send()?;
        
        if !response.status().is_success() {
            return Err(ExchangeError::HttpError(
                reqwest::Error::from(std::io::Error::new(
                    std::io::ErrorKind::Other, 
                    format!("Request failed with status: {}", response.status())
                ))
            ));
        }
        
        let response_text = response.text()?;
        
        // In a real implementation, you would parse the XML response
        // For this example, we'll return simulated stats
        // In a production environment, parse the XML response to get the actual values
        
        // Generate a deterministic UID validity based on folder name
        let uid_validity = folder_name.bytes().fold(0u32, |acc, b| acc.wrapping_add(b as u32));
        
        Ok(FolderStats {
            exists: 125,          // Total messages in folder
            recent: 5,            // New messages since last check
            unseen: 10,           // Unread messages
            uid_validity,         // A unique identifier for the folder state
            uid_next: 1000,       // Next UID to be assigned
        })
    }
    
    pub fn fetch_messages(&self, folder: &str, sequence_set: &str, items: &str) 
        -> Result<Vec<Message>, ExchangeError> {
        debug!("Fetching messages from folder '{}', sequence '{}', items '{}'", 
               folder, sequence_set, items);
        
        // Prepare headers
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("text/xml; charset=utf-8"));
        headers.insert(AUTHORIZATION, HeaderValue::from_str(self.token.as_ref().unwrap())?);
        
        // Parse sequence set (e.g., "1:10", "1,3,5", "*")
        let sequences = parse_sequence_set(sequence_set)?;
        
        // Determine folder ID
        let folder_id = match folder.to_uppercase().as_str() {
            "INBOX" => "inbox".to_string(),
            "SENT" | "SENT ITEMS" => "sentitems".to_string(),
            "DRAFTS" => "drafts".to_string(),
            "TRASH" | "DELETED ITEMS" => "deleteditems".to_string(),
            _ => folder.to_string(),
        };
        
        // Build the EWS FindItem request
        // In a real implementation, you would need to handle paging for large result sets
        let body = format!(r#"<?xml version="1.0" encoding="utf-8"?>
            <soap:Envelope xmlns:soap="http://schemas.xmlsoap.org/soap/envelope/"
                          xmlns:t="http://schemas.microsoft.com/exchange/services/2006/types">
              <soap:Body>
                <FindItem xmlns="http://schemas.microsoft.com/exchange/services/2006/messages"
                         xmlns:t="http://schemas.microsoft.com/exchange/services/2006/types"
                         Traversal="Shallow">
                  <ItemShape>
                    <t:BaseShape>IdOnly</t:BaseShape>
                    <t:AdditionalProperties>
                      <t:FieldURI FieldURI="item:Subject"/>
                      <t:FieldURI FieldURI="item:DateTimeReceived"/>
                      <t:FieldURI FieldURI="message:From"/>
                      <t:FieldURI FieldURI="message:IsRead"/>
                    </t:AdditionalProperties>
                  </ItemShape>
                  <IndexedPageItemView MaxEntriesReturned="100" Offset="0" BasePoint="Beginning"/>
                  <ParentFolderIds>
                    <t:DistinguishedFolderId Id="{}"/>
                  </ParentFolderIds>
                </FindItem>
              </soap:Body>
            </soap:Envelope>"#, folder_id);
        
        // Send the request
        let response = self.client
            .post(format!("{}/EWS/Exchange.asmx", self.base_url))
            .headers(headers)
            .body(body)
            .send()?;
        
        if !response.status().is_success() {
            return Err(ExchangeError::HttpError(
                reqwest::Error::from(std::io::Error::new(
                    std::io::ErrorKind::Other, 
                    format!("Request failed with status: {}", response.status())
                ))
            ));
        }
        
        let response_text = response.text()?;
        
        // In a real implementation, you would parse the XML response and build IMAP responses
        // For this example, we'll simulate messages
        
        // Parse the items requested (e.g., "BODY[HEADER] FLAGS UID")
        let fetch_items: Vec<&str> = items.trim_matches(|c| c == '(' || c == ')').split_whitespace().collect();
        
        let mut result = Vec::new();
        for &seq in &sequences {
            // Generate message data based on requested items
            let mut data_parts = Vec::new();
            
            for item in &fetch_items {
                match *item {
                    "FLAGS" => {
                        data_parts.push("FLAGS (\\Seen)".to_string());
                    },
                    "UID" => {
                        let uid = 1000 + seq;
                        data_parts.push(format!("UID {}", uid));
                    },
                    item if item.starts_with("BODY[HEADER]") => {
                        data_parts.push(format!("BODY[HEADER] {{320}}\r\nFrom: user{}@example.com\r\nTo: recipient@example.com\r\nSubject: Test message {}\r\nDate: Fri, 28 Mar 2025 10:{}:00 +0000\r\nMessage-ID: <{}.{}.{}@example.com>\r\n\r\n", 
                                               seq % 10, seq, seq % 60, seq, seq, seq));
                    },
                    item if item.starts_with("BODY[TEXT]") => {
                        data_parts.push(format!("BODY[TEXT] {{42}}\r\nThis is the body of test message {}.\r\n", seq));
                    },
                    item if item == "BODY[]" || item.starts_with("BODY[") => {
                        data_parts.push(format!("BODY[] {{362}}\r\nFrom: user{}@example.com\r\nTo: recipient@example.com\r\nSubject: Test message {}\r\nDate: Fri, 28 Mar 2025 10:{}:00 +0000\r\nMessage-ID: <{}.{}.{}@example.com>\r\n\r\nThis is the body of test message {}.\r\n", 
                                               seq % 10, seq, seq % 60, seq, seq, seq, seq));
                    },
                    _ => {
                        // Ignore unsupported items
                    }
                }
            }
            
            if !data_parts.is_empty() {
                let data = format!("({})", data_parts.join(" "));
                result.push(Message {
                    sequence: seq,
                    data,
                });
            }
        }
        
        Ok(result)
    }
}

// Helper function to parse an IMAP sequence set
fn parse_sequence_set(sequence_set: &str) -> Result<Vec<u32>, ExchangeError> {
    let mut result = Vec::new();
    
    for part in sequence_set.split(',') {
        if part == "*" {
            // For simplicity, treat "*" as "all messages" - in this case we'll return IDs 1-10
            for i in 1..=10 {
                result.push(i);
            }
        } else if part.contains(':') {
            // Range, e.g., "1:5"
            let range_parts: Vec<&str> = part.split(':').collect();
            if range_parts.len() != 2 {
                return Err(ExchangeError::ParseError(format!("Invalid range: {}", part)));
            }
            
            let start = if range_parts[0] == "*" {
                // In a real implementation, this would be the highest message number
                10
            } else {
                range_parts[0].parse::<u32>().map_err(|_| {
                    ExchangeError::ParseError(format!("Invalid sequence number: {}", range_parts[0]))
                })?
            };
            
            let end = if range_parts[1] == "*" {
                // In a real implementation, this would be the highest message number
                10
            } else {
                range_parts[1].parse::<u32>().map_err(|_| {
                    ExchangeError::ParseError(format!("Invalid sequence number: {}", range_parts[1]))
                })?
            };
            
            for i in start.min(end)..=start.max(end) {
                result.push(i);
            }
        } else {
            // Single message number
            let num = part.parse::<u32>().map_err(|_| {
                ExchangeError::ParseError(format!("Invalid sequence number: {}", part))
            })?;
            result.push(num);
        }
    }
    
    Ok(result)
}
