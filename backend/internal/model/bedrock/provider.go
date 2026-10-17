// Package bedrock implements AWS Bedrock ConverseStream using the official AWS SDK.
package bedrock

import (
	"context"
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"strings"
	"time"

	"github.com/aws/aws-sdk-go-v2/aws"
	"github.com/aws/aws-sdk-go-v2/config"
	"github.com/aws/aws-sdk-go-v2/credentials"
	"github.com/aws/aws-sdk-go-v2/service/bedrockruntime"
	"github.com/aws/aws-sdk-go-v2/service/bedrockruntime/document"
	"github.com/aws/aws-sdk-go-v2/service/bedrockruntime/types"
	modeltypes "rozsa/backend/internal/model/types"
)

type Provider struct{ client *bedrockruntime.Client }

func NewProvider(client *bedrockruntime.Client) *Provider { return &Provider{client: client} }
func (*Provider) API() modeltypes.API                     { return modeltypes.APIBedrockConverseStream }
func (*Provider) UsesExternalCredentials() bool           { return true }

func (provider *Provider) sdk(ctx context.Context, model modeltypes.Model) (*bedrockruntime.Client, error) {
	if provider.client != nil {
		return provider.client, nil
	}
	awsConfig, err := config.LoadDefaultConfig(ctx)
	if err != nil {
		return nil, fmt.Errorf("load AWS configuration: %w", err)
	}
	if awsConfig.Region == "" {
		awsConfig.Region = "us-east-1"
	}
	skipAuth := os.Getenv("AWS_BEDROCK_SKIP_AUTH") == "1"
	clientOptions := []func(*bedrockruntime.Options){func(bedrockOptions *bedrockruntime.Options) {
		if model.BaseURL != "" {
			bedrockOptions.BaseEndpoint = aws.String(model.BaseURL)
		}
		if skipAuth {
			bedrockOptions.AuthSchemePreference = []string{"sigv4"}
		}
	}}
	if skipAuth {
		awsConfig.Credentials = aws.NewCredentialsCache(credentials.NewStaticCredentialsProvider("dummy-access-key", "dummy-secret-key", ""))
	}
	return bedrockruntime.NewFromConfig(awsConfig, clientOptions...), nil
}

func (provider *Provider) StreamSimple(ctx context.Context, model modeltypes.Model, input modeltypes.Context, options modeltypes.SimpleStreamOptions, emit func(modeltypes.StreamEvent)) (err error) {
	if emit == nil {
		return errors.New("Bedrock stream requires an event emitter")
	}
	if options.Base.Timeout > 0 {
		var cancel context.CancelFunc
		ctx, cancel = context.WithTimeout(ctx, options.Base.Timeout)
		defer cancel()
	}
	output := modeltypes.AssistantMessage{API: modeltypes.APIBedrockConverseStream, Provider: model.Provider, Model: model.ID, Timestamp: time.Now().UnixMilli(), Content: []modeltypes.ContentBlock{}}
	defer func() {
		if err != nil {
			reason := modeltypes.Error
			if errors.Is(err, context.Canceled) {
				reason = modeltypes.Aborted
			}
			errorMessage := err.Error()
			failure := modeltypes.AssistantMessage{API: output.API, Provider: output.Provider, Model: output.Model, Timestamp: output.Timestamp, Content: []modeltypes.ContentBlock{}, StopReason: reason, ErrorMessage: &errorMessage}
			emit(modeltypes.StreamError{Reason: reason, Error: failure})
		} else {
			emit(modeltypes.Done{Reason: output.StopReason, Message: snapshot(output)})
		}
	}()
	client, err := provider.sdk(ctx, model)
	if err != nil {
		return err
	}
	request, err := buildRequest(model, input, options)
	if err != nil {
		return err
	}
	response, err := client.ConverseStream(ctx, request)
	if err != nil {
		return fmt.Errorf("Bedrock ConverseStream: %w", err)
	}
	defer response.GetStream().Close()
	emit(modeltypes.Start{Partial: snapshot(output)})
	blocks := map[int32]*streamBlock{}
	seenMessageStop := false
	for event := range response.GetStream().Events() {
		if _, isMessageStop := event.(*types.ConverseStreamOutputMemberMessageStop); isMessageStop {
			seenMessageStop = true
		}
		handleEvent(event, blocks, model, &output, emit)
	}
	if err := response.GetStream().Err(); err != nil {
		return fmt.Errorf("Bedrock event stream: %w", err)
	}
	if err := ctx.Err(); err != nil {
		return err
	}
	if !seenMessageStop {
		return errors.New("Bedrock event stream ended before messageStop")
	}
	return nil
}

func buildRequest(model modeltypes.Model, input modeltypes.Context, options modeltypes.SimpleStreamOptions) (*bedrockruntime.ConverseStreamInput, error) {
	messages, err := buildMessages(input)
	if err != nil {
		return nil, err
	}
	request := &bedrockruntime.ConverseStreamInput{ModelId: aws.String(model.ID), Messages: messages}
	cacheRetention := resolveCacheRetention(options.Base.CacheRetention)
	useCache := supportsPromptCaching(model)
	if input.SystemPrompt != nil && *input.SystemPrompt != "" {
		request.System = []types.SystemContentBlock{&types.SystemContentBlockMemberText{Value: *input.SystemPrompt}}
		if useCache {
			request.System = append(request.System, &types.SystemContentBlockMemberCachePoint{Value: buildCachePoint(cacheRetention)})
		}
	}
	maxTokens := model.MaxTokens
	if options.Base.MaxTokens != nil {
		maxTokens = *options.Base.MaxTokens
	}
	request.InferenceConfig = &types.InferenceConfiguration{MaxTokens: aws.Int32(int32(maxTokens))}
	if options.Base.Temperature != nil {
		request.InferenceConfig.Temperature = aws.Float32(float32(*options.Base.Temperature))
	}
	if len(input.Tools) > 0 {
		tools := make([]types.Tool, 0, len(input.Tools))
		for _, tool := range input.Tools {
			var schema any
			if len(tool.Parameters) > 0 {
				if err := json.Unmarshal(tool.Parameters, &schema); err != nil {
					return nil, fmt.Errorf("decode tool schema %s: %w", tool.Name, err)
				}
			}
			tools = append(tools, &types.ToolMemberToolSpec{Value: types.ToolSpecification{Name: aws.String(tool.Name), Description: aws.String(tool.Description), InputSchema: &types.ToolInputSchemaMemberJson{Value: document.NewLazyDocument(schema)}}})
		}
		request.ToolConfig = &types.ToolConfiguration{Tools: tools}
	}
	request.AdditionalModelRequestFields = additionalModelRequestFields(model, options)
	if useCache && len(request.Messages) > 0 {
		lastMessage := &request.Messages[len(request.Messages)-1]
		if lastMessage.Role == types.ConversationRoleUser {
			lastMessage.Content = append(lastMessage.Content, &types.ContentBlockMemberCachePoint{Value: buildCachePoint(cacheRetention)})
		}
	}
	return request, nil
}

func resolveCacheRetention(retention modeltypes.CacheRetention) modeltypes.CacheRetention {
	if retention != modeltypes.CacheDefault {
		return retention
	}
	if os.Getenv("ROZSA_CACHE_RETENTION") == "long" {
		return modeltypes.CacheLong
	}
	return modeltypes.CacheShort
}

func supportsPromptCaching(model modeltypes.Model) bool {
	if os.Getenv("AWS_BEDROCK_FORCE_CACHE") == "1" {
		return true
	}
	candidates := modelMatchCandidates(model)
	hasClaude := false
	for _, candidate := range candidates {
		if strings.Contains(candidate, "claude") {
			hasClaude = true
			break
		}
	}
	if !hasClaude {
		return false
	}
	for _, candidate := range candidates {
		if strings.Contains(candidate, "-4-") || strings.Contains(candidate, "claude-3-7-sonnet") || strings.Contains(candidate, "claude-3-5-haiku") {
			return true
		}
	}
	return false
}

func buildCachePoint(retention modeltypes.CacheRetention) types.CachePointBlock {
	cachePoint := types.CachePointBlock{Type: types.CachePointTypeDefault}
	if retention == modeltypes.CacheLong {
		cachePoint.Ttl = types.CacheTTLOneHour
	}
	return cachePoint
}

func additionalModelRequestFields(model modeltypes.Model, options modeltypes.SimpleStreamOptions) document.Interface {
	effort := options.Reasoning
	if effort == nil || !model.Reasoning || !isClaudeModel(model) {
		return nil
	}
	display := !isGovCloudModel(model)
	if supportsAdaptiveThinking(model) {
		thinking := map[string]any{"type": "adaptive"}
		if display {
			thinking["display"] = "summarized"
		}
		outputConfig := map[string]any{"effort": mapThinkingEffort(model, *effort)}
		return document.NewLazyDocument(map[string]any{"thinking": thinking, "output_config": outputConfig})
	}
	thinking := map[string]any{"type": "enabled", "budget_tokens": resolveThinkingBudget(*effort, options)}
	if display {
		thinking["display"] = "summarized"
	}
	return document.NewLazyDocument(map[string]any{"thinking": thinking, "anthropic_beta": []string{"interleaved-thinking-2025-05-14"}})
}

func modelMatchCandidates(model modeltypes.Model) []string {
	values := []string{model.ID}
	if model.Name != "" {
		values = append(values, model.Name)
	}
	var candidates []string
	for _, value := range values {
		normalized := strings.ToLower(value)
		candidates = append(candidates, normalized)
		replacer := strings.NewReplacer(" ", "-", "_", "-", ".", "-", ":", "-")
		normalized = replacer.Replace(normalized)
		if candidates[len(candidates)-1] != normalized {
			candidates = append(candidates, normalized)
		}
	}
	return candidates
}

func isClaudeModel(model modeltypes.Model) bool {
	for _, candidate := range modelMatchCandidates(model) {
		if strings.Contains(candidate, "claude") {
			return true
		}
	}
	return false
}
func isGovCloudModel(model modeltypes.Model) bool {
	id := strings.ToLower(model.ID)
	return strings.HasPrefix(id, "us-gov.") || strings.HasPrefix(id, "arn:aws-us-gov:")
}
func supportsAdaptiveThinking(model modeltypes.Model) bool {
	for _, candidate := range modelMatchCandidates(model) {
		if strings.Contains(candidate, "opus-4-6") || strings.Contains(candidate, "opus-4-7") || strings.Contains(candidate, "opus-4-8") || strings.Contains(candidate, "sonnet-4-6") {
			return true
		}
	}
	return false
}

func mapThinkingEffort(model modeltypes.Model, effort modeltypes.ThinkingEffort) string {
	if modelSpecificEffort, exists := model.ThinkingEffortMap[effort]; exists && modelSpecificEffort != nil {
		return *modelSpecificEffort
	}
	switch effort {
	case modeltypes.ThinkingOff, modeltypes.ThinkingLow:
		return "low"
	case modeltypes.ThinkingMedium:
		return "medium"
	case modeltypes.ThinkingHigh:
		return "high"
	case modeltypes.ThinkingXHigh:
		for _, candidate := range modelMatchCandidates(model) {
			if strings.Contains(candidate, "opus-4-7") || strings.Contains(candidate, "opus-4-8") {
				return "xhigh"
			}
		}
		return "high"
	case modeltypes.ThinkingMax:
		return "max"
	default:
		return "high"
	}
}

func resolveThinkingBudget(effort modeltypes.ThinkingEffort, options modeltypes.SimpleStreamOptions) uint64 {
	if budget, exists := options.ThinkingEffortBudgets[effort]; exists && effort != modeltypes.ThinkingOff {
		return budget
	}
	if effort == modeltypes.ThinkingXHigh {
		if budget, exists := options.ThinkingEffortBudgets[modeltypes.ThinkingHigh]; exists {
			return budget
		}
	}
	if effort == modeltypes.ThinkingMax {
		for _, fallback := range []modeltypes.ThinkingEffort{modeltypes.ThinkingXHigh, modeltypes.ThinkingHigh} {
			if budget, exists := options.ThinkingEffortBudgets[fallback]; exists {
				return budget
			}
		}
	}
	switch effort {
	case modeltypes.ThinkingOff:
		return 1024
	case modeltypes.ThinkingLow:
		return 2048
	case modeltypes.ThinkingMedium:
		return 8192
	default:
		return 16384
	}
}

type streamBlockKind uint8

const (
	streamBlockText streamBlockKind = iota
	streamBlockThinking
	streamBlockTool
)

type streamBlock struct {
	blockKind     streamBlockKind
	contentIndex  int
	contentText   string
	signature     string
	toolUseID     string
	toolName      string
	toolInputJSON string
	redacted      bool
}

func handleEvent(event types.ConverseStreamOutput, blocks map[int32]*streamBlock, model modeltypes.Model, output *modeltypes.AssistantMessage, emit func(modeltypes.StreamEvent)) {
	switch eventVariant := event.(type) {
	case *types.ConverseStreamOutputMemberContentBlockStart:
		blockIndex := aws.ToInt32(eventVariant.Value.ContentBlockIndex)
		if toolUseStart, isToolUse := eventVariant.Value.Start.(*types.ContentBlockStartMemberToolUse); isToolUse {
			block := &streamBlock{blockKind: streamBlockTool, contentIndex: len(output.Content), toolUseID: aws.ToString(toolUseStart.Value.ToolUseId), toolName: aws.ToString(toolUseStart.Value.Name)}
			blocks[blockIndex] = block
			output.Content = append(output.Content, modeltypes.ToolCallBlock{ID: block.toolUseID, Name: block.toolName, Arguments: json.RawMessage(`{}`)})
			emit(modeltypes.ToolCallStart{ContentIndex: block.contentIndex, Partial: snapshot(*output)})
		}
	case *types.ConverseStreamOutputMemberContentBlockDelta:
		blockIndex := aws.ToInt32(eventVariant.Value.ContentBlockIndex)
		switch delta := eventVariant.Value.Delta.(type) {
		case *types.ContentBlockDeltaMemberText:
			block := blocks[blockIndex]
			if block == nil {
				block = &streamBlock{blockKind: streamBlockText, contentIndex: len(output.Content)}
				blocks[blockIndex] = block
				output.Content = append(output.Content, modeltypes.TextBlock{})
				emit(modeltypes.TextStart{ContentIndex: block.contentIndex, Partial: snapshot(*output)})
			}
			block.contentText += delta.Value
			output.Content[block.contentIndex] = modeltypes.TextBlock{Text: block.contentText}
			emit(modeltypes.TextDelta{ContentIndex: block.contentIndex, Delta: delta.Value, Partial: snapshot(*output)})
		case *types.ContentBlockDeltaMemberToolUse:
			if block := blocks[blockIndex]; block != nil && block.blockKind == streamBlockTool {
				piece := aws.ToString(delta.Value.Input)
				block.toolInputJSON += piece
				emit(modeltypes.ToolCallDelta{ContentIndex: block.contentIndex, Delta: piece, Partial: snapshot(*output)})
			}
		case *types.ContentBlockDeltaMemberReasoningContent:
			reasoning, isReasoningText := delta.Value.(*types.ReasoningContentBlockDeltaMemberText)
			if isReasoningText {
				block := blocks[blockIndex]
				if block == nil {
					block = &streamBlock{blockKind: streamBlockThinking, contentIndex: len(output.Content)}
					blocks[blockIndex] = block
					output.Content = append(output.Content, modeltypes.ThinkingBlock{})
					emit(modeltypes.ThinkingStart{ContentIndex: block.contentIndex, Partial: snapshot(*output)})
				}
				block.contentText += reasoning.Value
				output.Content[block.contentIndex] = modeltypes.ThinkingBlock{Thinking: block.contentText, Redacted: block.redacted}
				emit(modeltypes.ThinkingDelta{ContentIndex: block.contentIndex, Delta: reasoning.Value, Partial: snapshot(*output)})
			} else if signature, isSignature := delta.Value.(*types.ReasoningContentBlockDeltaMemberSignature); isSignature {
				if block := blocks[blockIndex]; block != nil {
					block.signature += signature.Value
				}
			} else if _, isRedacted := delta.Value.(*types.ReasoningContentBlockDeltaMemberRedactedContent); isRedacted {
				block := blocks[blockIndex]
				if block == nil {
					block = &streamBlock{blockKind: streamBlockThinking, contentIndex: len(output.Content)}
					blocks[blockIndex] = block
					output.Content = append(output.Content, modeltypes.ThinkingBlock{})
					emit(modeltypes.ThinkingStart{ContentIndex: block.contentIndex, Partial: snapshot(*output)})
				}
				block.redacted = true
				output.Content[block.contentIndex] = modeltypes.ThinkingBlock{Thinking: block.contentText, Redacted: true}
			}
		}
	case *types.ConverseStreamOutputMemberContentBlockStop:
		blockIndex := aws.ToInt32(eventVariant.Value.ContentBlockIndex)
		block := blocks[blockIndex]
		if block == nil {
			return
		}
		delete(blocks, blockIndex)
		switch block.blockKind {
		case streamBlockText:
			emit(modeltypes.TextEnd{ContentIndex: block.contentIndex, Content: block.contentText, Partial: snapshot(*output)})
		case streamBlockThinking:
			var signature *string
			if block.signature != "" {
				signature = &block.signature
			}
			output.Content[block.contentIndex] = modeltypes.ThinkingBlock{Thinking: block.contentText, Signature: signature, Redacted: block.redacted}
			emit(modeltypes.ThinkingEnd{ContentIndex: block.contentIndex, Content: block.contentText, Partial: snapshot(*output)})
		case streamBlockTool:
			var arguments any
			if json.Unmarshal([]byte(block.toolInputJSON), &arguments) != nil {
				arguments = map[string]any{}
			}
			encodedArguments, _ := json.Marshal(arguments)
			tool := modeltypes.ToolCallBlock{ID: block.toolUseID, Name: block.toolName, Arguments: encodedArguments}
			output.Content[block.contentIndex] = tool
			emit(modeltypes.ToolCallEnd{ContentIndex: block.contentIndex, ToolCall: tool, Partial: snapshot(*output)})
		}
	case *types.ConverseStreamOutputMemberMessageStop:
		switch eventVariant.Value.StopReason {
		case types.StopReasonEndTurn, types.StopReasonStopSequence:
			output.StopReason = modeltypes.Stop
		case types.StopReasonMaxTokens, types.StopReasonModelContextWindowExceeded:
			output.StopReason = modeltypes.Length
		case types.StopReasonToolUse:
			output.StopReason = modeltypes.ToolUse
		default:
			output.StopReason = modeltypes.Error
		}
	case *types.ConverseStreamOutputMemberMetadata:
		if usage := eventVariant.Value.Usage; usage != nil {
			output.Usage.Input = uint64(aws.ToInt32(usage.InputTokens))
			output.Usage.Output = uint64(aws.ToInt32(usage.OutputTokens))
			output.Usage.TotalTokens = uint64(aws.ToInt32(usage.TotalTokens))
			if usage.CacheReadInputTokens != nil {
				output.Usage.CacheRead = uint64(*usage.CacheReadInputTokens)
			}
			if usage.CacheWriteInputTokens != nil {
				output.Usage.CacheWrite = uint64(*usage.CacheWriteInputTokens)
			}
			output.Usage.Cost = modeltypes.UsageCost{Input: model.Cost.Input / 1e6 * float64(output.Usage.Input), Output: model.Cost.Output / 1e6 * float64(output.Usage.Output), CacheRead: model.Cost.CacheRead / 1e6 * float64(output.Usage.CacheRead), CacheWrite: model.Cost.CacheWrite / 1e6 * float64(output.Usage.CacheWrite)}
			output.Usage.Cost.Total = output.Usage.Cost.Input + output.Usage.Cost.Output + output.Usage.Cost.CacheRead + output.Usage.Cost.CacheWrite
		}
	}
}

func snapshot(output modeltypes.AssistantMessage) modeltypes.AssistantMessage {
	cloneString := func(value *string) *string {
		if value == nil {
			return nil
		}
		cloned := *value
		return &cloned
	}
	output.ResponseID = cloneString(output.ResponseID)
	output.ResponseModel = cloneString(output.ResponseModel)
	output.ErrorMessage = cloneString(output.ErrorMessage)
	content := make([]modeltypes.ContentBlock, len(output.Content))
	for index, contentBlock := range output.Content {
		switch block := contentBlock.(type) {
		case modeltypes.TextBlock:
			block.Signature = cloneString(block.Signature)
			content[index] = block
		case modeltypes.ThinkingBlock:
			block.Signature = cloneString(block.Signature)
			content[index] = block
		case modeltypes.ToolCallBlock:
			block.Arguments = append([]byte(nil), block.Arguments...)
			content[index] = block
		default:
			content[index] = block
		}
	}
	output.Content = content
	return output
}

func buildMessages(input modeltypes.Context) ([]types.Message, error) {
	messages := make([]types.Message, 0, len(input.Messages))
	for _, message := range input.Messages {
		content := []types.ContentBlock{}
		role := types.ConversationRoleUser
		switch messageVariant := message.(type) {
		case modeltypes.UserMessage:
			switch value := messageVariant.Content.(type) {
			case modeltypes.UserText:
				content = append(content, &types.ContentBlockMemberText{Value: string(value)})
			case modeltypes.UserBlocks:
				for _, contentBlock := range value {
					switch block := contentBlock.(type) {
					case modeltypes.TextBlock:
						content = append(content, &types.ContentBlockMemberText{Value: block.Text})
					case modeltypes.ImageBlock:
						decodedImage, err := base64.StdEncoding.DecodeString(block.Data)
						if err != nil {
							decodedImage = []byte{}
						}
						format := types.ImageFormatPng
						switch strings.ToLower(block.MimeType) {
						case "image/jpeg", "image/jpg":
							format = types.ImageFormatJpeg
						case "image/gif":
							format = types.ImageFormatGif
						case "image/webp":
							format = types.ImageFormatWebp
						case "image/png":
						default:
							continue
						}
						content = append(content, &types.ContentBlockMemberImage{Value: types.ImageBlock{Format: format, Source: &types.ImageSourceMemberBytes{Value: decodedImage}}})
					}
				}
			}
		case modeltypes.AssistantMessage:
			role = types.ConversationRoleAssistant
			for _, contentBlock := range messageVariant.Content {
				if textBlock, isText := contentBlock.(modeltypes.TextBlock); isText {
					if strings.TrimSpace(textBlock.Text) != "" {
						content = append(content, &types.ContentBlockMemberText{Value: textBlock.Text})
					}
				}
				if toolCall, isToolCall := contentBlock.(modeltypes.ToolCallBlock); isToolCall {
					var arguments any
					if err := json.Unmarshal(toolCall.Arguments, &arguments); err != nil {
						return nil, err
					}
					content = append(content, &types.ContentBlockMemberToolUse{Value: types.ToolUseBlock{ToolUseId: aws.String(toolCall.ID), Name: aws.String(toolCall.Name), Input: document.NewLazyDocument(arguments)}})
				}
				if thinkingBlock, isThinking := contentBlock.(modeltypes.ThinkingBlock); isThinking && strings.TrimSpace(thinkingBlock.Thinking) != "" {
					if thinkingBlock.Signature != nil && strings.TrimSpace(*thinkingBlock.Signature) != "" {
						content = append(content, &types.ContentBlockMemberReasoningContent{Value: &types.ReasoningContentBlockMemberReasoningText{Value: types.ReasoningTextBlock{Text: aws.String(thinkingBlock.Thinking), Signature: thinkingBlock.Signature}}})
					} else {
						content = append(content, &types.ContentBlockMemberText{Value: thinkingBlock.Thinking})
					}
				}
			}
		case modeltypes.ToolResultMessage:
			toolResultContent := []types.ToolResultContentBlock{}
			for _, contentBlock := range messageVariant.Content {
				if textBlock, isText := contentBlock.(modeltypes.TextBlock); isText {
					toolResultContent = append(toolResultContent, &types.ToolResultContentBlockMemberText{Value: textBlock.Text})
				}
			}
			status := types.ToolResultStatusSuccess
			if messageVariant.IsError {
				status = types.ToolResultStatusError
			}
			toolResult := &types.ContentBlockMemberToolResult{Value: types.ToolResultBlock{ToolUseId: aws.String(messageVariant.ToolCallID), Content: toolResultContent, Status: status}}
			if previousMessage := len(messages) - 1; previousMessage >= 0 && messages[previousMessage].Role == types.ConversationRoleUser {
				messages[previousMessage].Content = append(messages[previousMessage].Content, toolResult)
				continue
			}
			content = append(content, toolResult)
		}
		if len(content) > 0 {
			messages = append(messages, types.Message{Role: role, Content: content})
		}
	}
	return messages, nil
}
